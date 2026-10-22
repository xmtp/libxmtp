# Review guide

This file says what to weigh and report in a libxmtp review, for people and
AI reviewers. `AGENTS.md` files and `.agents/skills/` say how to work;
`reviewing-specs` covers a spec change. Approved specs in `docs/specs/` win
over code, plans, and this file; `docs/specs/README.md` says which spec owns
what. A review tool's own instructions set its scope and verdict; this file
sets shared priorities within that scope.

## Approach

- **Intent.** Does the diff do what the description says, and only that?
  Should the change exist?
- **Layer.** Shared behavior belongs in the Rust core and `crates/xmtp_sdk`.
  The `sdks/*` packages wrap it; logic added to one SDK drifts from the
  others. Is an existing helper or type reused?
- **Spec.** Does the change match the owning spec? Can a guarded case happen at
  all? A spec invariant can rule out a local state, never bad input from the
  backend or another client.
- **Reach.** Look past the diff: callers, native and WASM, each SDK, the other
  PRs in the stack, and docs or `AGENTS.md` files the change makes stale.
- **Gaps.** Are tests, error and cancellation paths, a migration, or a spec
  update missing? Would each new test fail if the behavior broke? Is a
  performance claim measured with EXPLAIN output or a benchmark?

## Risk areas, most harmful first

- **Ordering and recovery (PROC, SEND).** On a group or identity topic, a
  client applies or terminally rejects the head envelope before later ones.
  Work it cannot handle yet is held, never skipped: a skipped commit forks the
  group. Missing state or a down verifier is unavailable, not invalid, because
  a rejection is final. Watch error mappings that turn one into the other.
  Effects and the stored position are saved in one transaction. Delivery is
  acknowledged only after the app accepts the item. While a publish outcome is
  unknown, the prepared bytes are kept, never re-encrypted.
- **MLS validation and permissions (GMOD, JOIN, FORK, PERM).** Every member
  must reach the same decision from its own state; a rule that one version,
  platform, or path applies differently forks the group. A failed validation
  leaves group state and epoch unchanged.
- **Identity and authentication (IDENT, AUTH).** The client derives
  association state itself and does not trust backend acceptance. An
  identity-update signature is used once. Does a new API, SDK, or binding path
  keep the validation of the old path? Does an error path fail open?
- **Keys, secrets, and privacy.** A key deleted too early loses a Welcome in
  flight; one kept too long widens the compromise window (JOIN). Keys,
  credentials, and auth headers stay out of logs, `Debug` output, errors, and
  telemetry. Logs also carry no payloads or full ids.
- **Wire, stored data, and SDK contracts (`proto/`, API, ARCH, CTYPE).**
  Supported clients, backends, and SDKs upgrade at different times. CI runs
  `buf lint`, not `buf breaking`, so only a reviewer catches a reused or
  retyped field. Migrations are append-only. An old client must not fail on an
  archive element or content type it lacks.
- **Concurrency and lifetime.** What owns each task, and what happens on drop,
  cancel, panic, restart, and partial failure? Can a waiter hang, or a held
  lock wait on work that needs the same lock? Release builds abort on panic, so
  a panic on remote input crashes the app.
- **Backend and CI.** Retention never deletes envelopes that later messages
  depend on (OPS). Telemetry carries no request data. Check workflow token
  scope, cache write access, and each new dependency.

## What blocks

- A defect this PR introduces or worsens that gives wrong user-visible
  results, data loss, wrong ordering, a security or privacy breach, or a break
  in supported compatibility that another party can observe. Draft spec status
  does not excuse these harms.
- A change to an externally observable promise with no spec amendment for the
  owner to review. A new helper, API method, or option alone does not need one.

A PR that removes or weakens tests, validation, log redaction, or CI checks
needs a stated reason and a human reviewer; if it causes a harm above, it also
blocks. Everything else does not block. Label a minor point "Nit:" or leave it
out. A defect the base already has is pre-existing and does not block. Do not
ask for perfection.

## Evidence

- Give the file and line, the input or state, and the wrong result. Name the
  spec or rule it breaks, and show that real callers reach the path.
- A claim about language, library, or tool behavior needs a source or a test.
  Past findings were wrong about JS regex, TypeScript visibility, UniFFI
  cancellation, atomic ordering, and Nix.
- An unverified concern is a question, never a blocking finding. Report one
  root cause once.

## Do not raise

- What `just lint` fails on: formatting, Clippy, Oxlint, markdownlint,
  `buf lint`, `cargo deny` bans, licenses, and sources, hakari, and
  `just spec-check` errors.
- Generated files, such as `schema_gen.rs`, the `.sqlx` cache, and
  `docs/error_glossary.md`. Review the source; a hand edit to generated output
  is the finding.
- A risk that a spec's Known limitations or `docs/specs/waivers.toml` records,
  unless this PR widens it. A `gap` waiver records wrong behavior: when this PR
  changes that code, the normal rules apply.
- A finding that belongs to another PR in the stack.
- A finding already answered with an invariant, waiver, or owner decision that
  you verified, unless the code changed.
- Naming, layout, or refactor preferences with no behavior or upkeep cost.
