---
name: working-with-nix
description: Use when working with Nix flakes, selecting devShells, debugging environment issues, or understanding Nix infrastructure - covers shell selection, environment detection, common commands, and project-specific constraints like pinned Rust versions
---

# Working with Nix in libxmtp

This skill helps with Nix development environments in the libxmtp repository.

## Quick Shell Selection

| Task | Shell | Command |
|------|-------|---------|
| General Rust development | default | `nix develop` |
| Android builds/testing | android | `nix develop .#android` |
| iOS builds/testing | ios | `nix develop .#ios` |
| JavaScript/Node.js | js | `nix develop .#js` |
| WebAssembly builds | wasm | `nix develop .#wasm` |

## Environment Detection

Check if you're in a Nix shell:
```bash
# Set by the default shell
echo $XMTP_NIX_ENV  # "yes" if in default shell

# Works for any Nix shell
[[ -n "$IN_NIX_SHELL" ]] && echo "In Nix shell"
```

## Critical Constraints

### Rust Version is Pinned - Do Not Change

The project uses **Rust 1.92.0** pinned via static manifest:
```nix
# flake.nix
rust-manifest = {
  url = "https://static.rust-lang.org/dist/channel-rust-1.92.0.toml";
  flake = false;
};
```

All shells use `xmtp.mkToolchain` which reads from this manifest. Never modify this URL without project-wide coordination.

### iOS Shell is macOS Only

The `.#ios` shell only works on macOS (Darwin). It requires:
- `xcbuild`
- `darwin.cctools`
- Apple-specific LLVM toolchains

Attempting to use it on Linux will fail.

### Untracked Files are Invisible to Flakes

Nix flakes only see files tracked by git. If a new file isn't building:
```bash
git add <new-file>  # Then rebuild
```

### Cachix is Essential for Build Times

First-time builds without Cachix can exceed 30 minutes. Configure once:
```bash
# The flake already has extra-substituters configured
# Just ensure you can access xmtp.cachix.org

# If builds are slow, verify with:
nix config show | grep substituters
```

The flake's `nixConfig` already includes:
```nix
extra-trusted-public-keys = "xmtp.cachix.org-1:...";
extra-substituters = "https://xmtp.cachix.org";
```

## Essential Commands

```bash
# Enter shells
nix develop              # Default shell
nix develop .#android    # Android shell
nix develop .#ios        # iOS shell (macOS only)
nix develop .#js         # JavaScript shell
nix develop .#wasm       # WASM shell

# Check what's available
nix flake show           # List all outputs
nix flake metadata       # Show inputs and their versions

# Update dependencies
nix flake update         # Update all inputs
nix flake lock --update-input nixpkgs  # Update specific input

# Build WASM bindings as a package
nix build .#wasm-bindings

# Debug
nix develop --show-trace  # Verbose error output
nix repl .                # Interactive exploration
```

## Using with direnv

The project includes `.envrc` with `use flake .` which auto-loads the default shell.

```bash
# First time setup
direnv allow

# If direnv isn't loading
direnv reload

# Check status
direnv status
```

Note: direnv only loads the default shell. For platform-specific work (Android, iOS, WASM), manually enter that shell with `nix develop .#<shell>`.

## When NOT to Invoke This Skill

- Simple `cargo` commands that work fine in the current environment
- Reading/editing Nix files when the syntax is straightforward
- General Rust questions unrelated to Nix tooling

## Platform Support

| Platform | Shells Available |
|----------|------------------|
| macOS (aarch64-darwin) | default, android, ios, js, wasm |
| Linux (x86_64-linux) | default, android, js, wasm |

The flake defines `systems = ["aarch64-darwin" "x86_64-linux"]`.

## Key Files

- `flake.nix` - DevShell definitions, input pinning, cachix config
- `nix/libxmtp.nix` - Default shell tools
- `nix/android.nix` - Android shell with NDK
- `nix/ios.nix` - iOS shell (macOS only)
- `nix/js.nix` - JavaScript shell with Playwright
- `nix/package/wasm.nix` - WASM shell and package build
- `nix/lib/mkToolchain.nix` - Rust version pinning logic
- `dev/nix-up` - Installation script for Nix and direnv

## Further Reference

- [Shell Reference](shell-reference.md) - Detailed tool inventories for each shell
- [Troubleshooting](troubleshooting.md) - Common issues and solutions
