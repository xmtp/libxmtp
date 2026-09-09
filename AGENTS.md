# libxmtp

Rust workspace. MLS messaging. Bindings: `bindings/{mobile,node,wasm}`. SDKs: `sdks/{android,ios,js}`.

## Read first

- `docs/self-hosted/project.md` — scope, phases.
- `docs/self-hosted/guidelines.md` — hard rules. They win.
- `docs/self-hosted/style-guide.md` — code and doc style.
- `docs/specs/` — approved specs.
- Skills in `.claude/skills/`: `writing-rust-tests`, `working-with-nix`.

## Commands

Every `just` recipe runs inside `nix develop` (`dev/nix-shell`).
Never run `cargo`, `yarn`, `./gradlew`, or `swift` bare. Use `just`, or `dev/nix-shell '<cmd>'`.

```bash
just                    # list all recipes
just backend up         # docker services. Most tests need them.
just backend build      # self-hosted service through Nix. No database needed.
just backend db-up      # disposable PostgreSQL 18 for backend tests.
just backend sql-prepare # migrate test DB and refresh checked SQL metadata.
just backend sql-check  # verify checked SQL metadata against test DB.
just backend test       # backend unit and RPC/storage tests.
just check              # cargo check. default-members only.
just test               # workspace tests. default-members only.
just lint               # rust + config + markdown. Run before commit.
just lint-markdown      # excludes generated error glossary and JS release changelogs.
just lint-proto         # Buf checks the local proto/ schemas.
just validation         # isolated shared validation checks and native/wasm tests.
just docs build         # Starlight site. Run just docs install first.
just docs lint          # site code and Markdown.
just docs format-check  # site formatting.
just docs test          # site build-tool tests.
```

The SDK stack contains `db`, `backend`, `anvil`, and `toxiproxy`.
Use `XMTP_BACKEND_URL=http://127.0.0.1:5050` when local IPv6 forwarding fails.
`just test` excludes backend database tests; run them with `just backend test`.

`default-members` = `apps/backend`, `bindings/*`, `crates/*`. Other apps: see their `AGENTS.md`.

## Rules

- Tests use `#[xmtp_common::test(unwrap_try = true)]`. Never `#[test]`.
- Every package has an `AGENTS.md`. Read it before working there. Update it when its commands change.
- `CLAUDE.md` is only a pointer (`@AGENTS.md`). Content goes in `AGENTS.md`.
