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
just check              # cargo check. default-members only.
just test               # v3 + d14n tests. default-members only.
just lint               # rust + config + markdown. Run before commit.
just lint-markdown      # excludes generated error glossary and JS release changelogs.
```

`default-members` = `apps/mls_validation_service`, `bindings/*`, `crates/*`. Other apps: see their `AGENTS.md`.

## Rules

- Tests use `#[xmtp_common::test(unwrap_try = true)]`. Never `#[test]`.
- Every package has an `AGENTS.md`. Read it before working there. Update it when its commands change.
- `CLAUDE.md` is only a pointer (`@AGENTS.md`). Content goes in `AGENTS.md`.
