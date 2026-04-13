# Extra Traefik Routes — Design Spec

## Overview

Add support for user-defined Traefik routes in xnet's TOML configuration. This allows the NixOS deployment to inject additional routes (e.g., a status page) into Traefik's dynamic config alongside the auto-generated service routes.

## Motivation

The xnet Hetzner deployment runs a status page via Caddy on an internal port (8899). Traefik owns port 80 and handles all incoming HTTP/gRPC traffic. We need Traefik to route `Host(`migrate.xmtp.run`)` to the status page without putting another proxy (like Caddy or nginx) in front of Traefik, which breaks gRPC.

## TOML Configuration

New top-level array of tables in `xnet.toml`:

```toml
[[extra_traefik_routes]]
name = "status-page"
rule = "Host(`migrate.xmtp.run`)"
url = "http://host.docker.internal:8899"
priority = 100

[[extra_traefik_routes]]
name = "another-service"
rule = "Host(`other.example.com`)"
url = "http://host.docker.internal:9999"
```

### Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Unique identifier for the router and service in Traefik's dynamic config. Must be a valid Traefik service name (alphanumeric, hyphens, underscores). |
| `rule` | string | yes | Traefik matcher rule. Passed through verbatim. Any valid [Traefik router rule](https://doc.traefik.io/traefik/routing/routers/#rule) works: `Host(...)`, `PathPrefix(...)`, `Headers(...)`, combinations with `&&`/`||`. |
| `url` | string | yes | Backend URL. Where Traefik forwards matching requests. Supports `http://`, `https://`, and `h2c://` schemes. **Important:** This URL must be reachable from inside the Traefik Docker container (see URL Addressing below). |
| `priority` | integer | no | Traefik router priority. Higher values match first. If omitted, Traefik uses its default priority (based on rule length). Useful to ensure extra routes take precedence over wildcard service routes. |

### URL Addressing

The `url` field is used by Traefik inside its Docker container. This means:

- **`127.0.0.1` / `localhost`** refers to the Traefik container itself, NOT the host machine. Do not use these for host services.
- **`host.docker.internal`** resolves to the host machine (Docker Engine 20.10+, Linux and macOS). The Traefik container is configured with `extra_hosts: ["host.docker.internal:host-gateway"]` to enable this.
- **Docker container names** (e.g., `xnet-toxiproxy`) are reachable directly by name on the `xnet` network.

For services running on the host (like a Caddy status page on port 8899), use:
```toml
url = "http://host.docker.internal:8899"
```

For services running in Docker on the xnet network, use the container name:
```toml
url = "http://my-container:8080"
```

### Defaults

If `[[extra_traefik_routes]]` is omitted entirely, no extra routes are added. The existing auto-generated service routes are unaffected.

## Implementation

### Rust changes (xnet-cli)

**File: `apps/xnet/lib/src/config/toml_config.rs`**

Add a new struct and field to `TomlConfig`:

```rust
#[derive(Deserialize, Default, Debug, Clone)]
pub struct ExtraTraefikRoute {
    pub name: String,
    pub rule: String,
    pub url: String,
    pub priority: Option<i32>,
}
```

Add to `TomlConfig`:

```rust
#[serde(default)]
pub extra_traefik_routes: Vec<ExtraTraefikRoute>,
```

**File: `apps/xnet/lib/src/config/loadable.rs`**

Propagate extra routes from `TomlConfig` to `Config`. Add a new field to the `Config` struct:

```rust
/// Extra Traefik routes from TOML config
#[builder(default)]
pub extra_traefik_routes: Vec<ExtraTraefikRoute>,
```

And in `Config::load()`, wire it through the builder (around line 178):

```rust
.extra_traefik_routes(toml.extra_traefik_routes)
```

**File: `apps/xnet/lib/src/services/traefik.rs`**

Add `host.docker.internal` to the Traefik container's `HostConfig` so it can reach host services:

```rust
extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
```

**File: `apps/xnet/lib/src/services/traefik_config.rs`**

The current `TraefikConfig` stores routes as `HashMap<String, u16>` (hostname -> ToxiProxy port), which cannot represent extra routes with arbitrary URLs and matcher rules. Add a separate `Vec` for extra routes:

```rust
pub struct TraefikConfig {
    config_path: PathBuf,
    /// Hostname -> ToxiProxy port mapping (auto-generated service routes)
    routes: Arc<Mutex<HashMap<String, u16>>>,
    /// User-defined extra routes (from TOML config, stored in memory only)
    extra_routes: Arc<Mutex<Vec<ExtraTraefikRoute>>>,
}
```

Extra routes are **not** round-tripped through the YAML file. The current `load_from_file()` parses routes by extracting hostnames from `Host(...)` rules and ports from URLs — extra routes with arbitrary rules/URLs would be silently discarded on reload. Instead, extra routes are stored only in memory from the TOML config and re-merged into `dynamic.yml` on every `write()` call.

Update `write()` to merge both route types into the YAML output. Add `set_extra_routes()`:

```rust
pub fn set_extra_routes(&self, routes: Vec<ExtraTraefikRoute>) -> Result<()> {
    {
        let mut extra = self.extra_routes.lock().unwrap();
        *extra = routes;
    }
    self.write()?;
    Ok(())
}
```

**File: `apps/xnet/lib/src/app/service_manager.rs`**

After creating `traefik_config` (line 84), use the existing `config` variable (already loaded at line 67):

```rust
let traefik_config = TraefikConfig::new(traefik.dynamic_config_path())?;
if !config.extra_traefik_routes.is_empty() {
    traefik_config.set_extra_routes(config.extra_traefik_routes.clone())?;
}
```

### Dynamic config output

For a route like:

```toml
[[extra_traefik_routes]]
name = "status-page"
rule = "Host(`migrate.xmtp.run`)"
url = "http://host.docker.internal:8899"
priority = 100
```

The generated `dynamic.yml` should include:

```yaml
http:
  routers:
    status-page:
      rule: "Host(`migrate.xmtp.run`)"
      service: status-page
      priority: 100
  services:
    status-page:
      loadBalancer:
        servers:
          - url: "http://host.docker.internal:8899"
        passHostHeader: true
```

Extra routes are merged with auto-generated service routes in the same `dynamic.yml`. Name collisions between extra routes and auto-generated routes are unlikely in practice: auto-generated routes use sanitized hostnames as keys (e.g. `node100_xmtpd_local`), while extra routes use user-chosen names (e.g. `status-page`). If a collision does occur, extra routes are inserted after auto-generated routes in `write()`, so they overwrite the auto-generated entry in the `HashMap` — but this is coincidental, not a guaranteed ordering. To avoid surprises, choose extra route names that don't resemble sanitized hostnames.

### NixOS changes (xnet-public)

**File: `modules/xnet.nix`**

Add a new option:

```nix
extraTraefikRoutes = lib.mkOption {
  type = lib.types.listOf (lib.types.submodule {
    options = {
      name = lib.mkOption { type = lib.types.str; };
      rule = lib.mkOption { type = lib.types.str; };
      url = lib.mkOption { type = lib.types.str; };
      priority = lib.mkOption {
        type = lib.types.nullOr lib.types.int;
        default = null;
      };
    };
  });
  default = [];
  description = "Additional Traefik routes to inject into the dynamic config";
};
```

Add to TOML generation:

```nix
extra_traefik_routes = map (r:
  { inherit (r) name rule url; }
  // lib.optionalAttrs (r.priority != null) { inherit (r) priority; }
) cfg.settings.extraTraefikRoutes;
```

**File: `modules/xnet-status/default.nix`**

Remove the `xnet-status-route` systemd service. Instead, configure the route declaratively:

```nix
services.xnet.settings.extraTraefikRoutes = [{
  name = "status-page";
  rule = "Host(`${cfg.domain}`)";
  url = "http://host.docker.internal:8899";
  priority = 100;
}];
```

## Testing

1. **Local (no extra routes)**: Run xnet-cli with a config that has no `[[extra_traefik_routes]]`. Verify `dynamic.yml` contains only auto-generated service routes.
2. **Local (with extra routes)**: Add a route to the TOML config. Verify it appears in `dynamic.yml` alongside service routes.
3. **Hetzner deploy**: Provision with the status page route configured. Verify `http://migrate.xmtp.run` serves the status page and `http://xnet-100.5-161-27-26.sslip.io` routes to xmtpd.
4. **Priority**: Verify that the extra route with `priority = 100` takes precedence over any auto-generated wildcard routes that might also match.
5. **host.docker.internal**: Verify that `host.docker.internal` resolves correctly from inside the Traefik container on Linux (`docker exec xnet-traefik ping -c1 host.docker.internal`).
