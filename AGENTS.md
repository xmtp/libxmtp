# libxmtp

Rust MLS workspace; platform SDKs live in `sdks/`.

- Read the relevant specs via [docs/specs/README.md](docs/specs/README.md).
  Approved specs win over code, plans, and XIPs. For reviews, read [REVIEW.md](REVIEW.md).
- Read `AGENTS.md` files along the path you change. Keep shared rules in the
  nearest common parent, local constraints here, and detailed procedures in linked
  docs. Update instructions when commands change. `CLAUDE.md` is only `@AGENTS.md`.
- Use `.agents/skills/writing-typescript` for TypeScript and JavaScript work.
- For agent instruction changes, use the [writing-agent-docs](.agents/skills/writing-agent-docs/SKILL.md) skill.
- For requested full-repo security audits, use the [security-hardening](.agents/skills/security-hardening/SKILL.md) skill.
- Branch from `origin/main`; target `main`.
- Spec requirement changes need an approved Ref plan before implementation;
  routine docs, CI, and tooling changes do not. Required plans describe public
  `sdks/*` type changes; update the plan if new surface emerges.
- Default to constants; a configuration knob needs a caller requiring a different value.
- Before expanding scope to handle a speculative or extremely unlikely race or
  security edge case, describe how it could fail, how likely it is, and the added
  complexity. Get the user's agreement before implementing that extra complexity.

## Commands

Run from the repository root through `dev/nix-shell`; never run `cargo`, `pnpm`,
`./gradlew`, or `swift` outside Nix. Prefer Just recipes. For Cargo options without
one, use `dev/nix-shell 'dev/agent-run cargo <command> ...'`.
For shell diagnostics and runtime maintenance, read [Nix guidance](nix/AGENTS.md).

```sh
dev/nix-shell 'just'                           # discover recipes
dev/nix-shell 'just check crate <name>'
dev/nix-shell 'just test crate <name>'
dev/nix-shell 'just test workspace -p <name> <args>' # nextest filters/options
dev/nix-shell 'just lint'
```

Default members are `apps/backend`, `apps/db_tools`, and `crates/*`; other apps need explicit checks.
Start `just backend up` through Nix before workspace tests; client tests need its
node, not only the databases. Backend database tests use `just backend test`.
Use `just backend status` for worktree addresses, never assume main-checkout ports.
Read `.agents/skills/working-with-worktrees` when creating or using another worktree.

Before broad source reads, use `just outline <path>`; for a known symbol use
`just show <file> <name>` (both through Nix). Fall back to `rg` for missed symbols;
use Serena for Rust references/types. See [agent tools](docs/agent-tools.md) for
setup and output recovery (`XMTP_RTK=0`).
For CI results, use `just ci-status <pr>` / `just ci-failures <job>` through Nix
and `.agents/skills/check-ci`, never raw log fetches.

## Tests

- Rust tests use `#[xmtp_common::test(unwrap_try = true)]`; dependency-cycle
  exceptions are documented locally.
- Prove every new test fails: break the behavior or restore the weaker
  implementation, run it and observe failure, then restore the code and pass.
- Tooling/CI checks and service troubleshooting: [dev/README.md](dev/README.md).
