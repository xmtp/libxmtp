# Codex agent tools

The project configuration in `.codex/config.toml` provides a Nix shell hook and
read-only Rust tools through Serena. Start Codex from this worktree. Review and
trust the project and its hook through the normal Codex trust flow. Restart an
existing session to load the configuration. The hook needs host `python3` to
enter Nix. The Serena runtime uses the Python and uv versions from Nix.

## Nix commands

The hook runs shell commands with `bash -c` through `dev/nix-shell`. Use Bash
syntax for hooked commands. It does not add `errexit` or `nounset` to them.
It defaults `XMTP_RTK` to `1` for supported command filters. An explicit value,
including an empty value or `0`, takes precedence.
It preserves the command text,
working directory, exit status, and permission checks. Commands outside this
worktree are not wrapped. This does not change the sandbox or approval policy.
Codex requires `PreToolUse.permissionDecision: allow` to accept a command
rewrite. This hook does not handle `PermissionRequest` or grant an escalation.
See the [Codex hook contract](https://learn.chatgpt.com/docs/hooks).
Nix still needs access to its daemon socket and store. If the sandbox blocks
that access, use an approved environment that permits Nix. Do not disable the
sandbox to make the hook work.

Keep using the explicit wrapper when the hook is unavailable:

```sh
dev/nix-shell 'just check'
dev/nix-shell --shell rust --command rustc --version
dev/nix-shell --command printf '%s\n' 'one argument'
```

The default shell contains the tools needed by Just script recipes. Just modules
can select a different shell. Do not set `NIX_DEVSHELL=rust` globally: that can
override module shell selection. The wrapper reuses a nested environment only
when the worktree, shell selection, and shell inputs match. It enters Nix again
after a relevant input changes. It does not infer reuse from `IN_NIX_SHELL` or
from a direnv environment. Separate top-level commands still enter Nix.

## Code navigation

Use these recipes inside `dev/nix-shell`:

```sh
dev/nix-shell 'just outline crates/xmtp_mls/src/client.rs'
dev/nix-shell 'just show sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt create'
```

`just outline` accepts one or more paths and prints declarations with
line ranges. `just show` accepts a file and one or more symbol names. Use a
qualified name when a short name is ambiguous; nesting is significant. For
example, Kotlin companion methods are under `Client.Companion`.

The `dev/ast-outline` launcher runs ast-outline 1.9.0 with Nix-provided Python in
the locked agent environment. Its first call can download dependencies. Both
navigation recipes preserve argument boundaries, including paths with spaces.
The outline recipe uses upstream defaults, including documentation, fields,
and attributes. Pass `--no-docs --no-fields --no-attrs` when smaller output is
useful. Use the launcher directly for other upstream options or JSON output.

Outlines are syntax-based and can miss macro-generated code or declarations in
files with parse errors. An absent symbol is not proof that it does not exist.
Use `rg` and focused source reads when needed. Some ast-outline user errors,
including missing symbols, print a note and return zero: inspect output as well
as status. Do not apply a second RTK filter to outlines or symbol source.

## Compact command output

The default and Rust Nix shells provide RTK 0.49.0. `dev/agent-run` applies its
dedicated Cargo filters inside selected recipes when `XMTP_RTK=1`. The Codex
hook sets this default; other agents can opt in explicitly:

```sh
XMTP_RTK=1 dev/nix-shell 'just check'
XMTP_RTK=1 dev/nix-shell 'just backend test --lib config'
XMTP_RTK=0 dev/nix-shell 'just check'
```

The runner supports Cargo build, check, Clippy, and test. It does not enter Nix
itself; direct use must remain inside `dev/nix-shell`. Root check/lint recipes
and backend tests use it. Nix package builds retain their original execution.
When `CI` is nonempty, RTK is missing, or compact output is not enabled, the
original command runs. Metadata, coverage, structured-output options, help,
test listing, and uncaptured output run raw. Custom `CARGO_TEST_CMD` values are
unchanged. No CI workflow needs an RTK installation or setting.

Nextest filtering is off by default. To try it, set both `XMTP_RTK=1` and
`XMTP_RTK_NEXTEST=1`. RTK 0.49.0 can hide short errors before a Nextest run starts.
The runner preserves failure status and prints a recovery instruction even
when RTK prints no diagnostic. This extra opt-in does not apply to CI.

Follow a printed RTK recovery hint first. If needed and safe, repeat the same
recipe with `XMTP_RTK=0`, preserving its arguments and environment. The runner
never retries automatically. Do not weaken a check or remove a tool option to
make filtering work. Global RTK recall settings are not changed by this repo.

Use `rtk gain --project` to inspect estimated savings. Do not use `rtk just`,
`rtk test just ...`, or a generic output pipeline around these recipes: those
do not apply the dedicated child-command filters and can hide useful output.

Run `dev/nix-shell 'just agent-test'` to verify routing, hook behavior, and
navigation against the locked dependencies. These tests do not start Serena or
backend services.

## Rust navigation

Codex starts `dev/serena` as an MCP stdio client. The launcher starts one shared
Serena server per worktree. Additional clients use the same server and analyzer.
Each worktree has its own loopback port, access token, lock, log, and cache under
`.cache/agents/serena`. The server remains running after clients disconnect.
Serena serializes tool calls. It is pinned to the tested commit in
`dev/agents/pyproject.toml`; `uv.lock` pins its dependencies.

The tools provide symbols, outlines, references, declarations with type
information, and advisory diagnostics. Editing and project-switching tools are
not exposed. File edits use the agent's normal file tools. Rust is the only
configured language. Other language servers need separate validation.

The launcher resolves `rust-analyzer` from Nix and sets its path explicitly.
Build scripts and procedural macros are enabled with `SQLX_OFFLINE=true`.
Save-triggered Cargo checks are disabled. Analysis can still run build scripts
and read dependencies. These are not sandboxed by the read-only tool list.

Diagnostics do not replace compiler checks. The spike detected a type mismatch,
but missed a borrow error in fast mode. Even compiler-backed diagnostics can
initially return an empty result. Run focused checks and tests before you finish
a code change. Cold workspace analysis can take tens of seconds and several
GiB of memory. Do not start a separate analyzer for each agent.

Stop the shared server when you finish, or before you reconnect after changing
the launcher, dependencies, Nix inputs, or server configuration:

```sh
dev/serena stop
```

This disconnects all clients in this worktree. Other worktrees are not affected.
If clients prevent graceful shutdown for 30 seconds, the stop command kills the
server and its child processes.
If startup fails, read `.cache/agents/serena/server.log`. The first start needs
network access to fetch the pinned Python dependencies. The launcher does not
install Rust or change global Codex or Serena settings.

## Ref on a Murmur VM

Nothing in this repository configures the Ref plan server for a local
developer. Configure it however you normally do.

On a Murmur agent VM, `.murmur/hooks/prepare-workspace.sh` writes the agent's
own `~/.claude.json` and `~/.codex/config.toml` after the repo is cloned and
before the agent starts. It reads `REF_API_KEY`, which arrives from
`murmur secret mount REF_API_KEY`, and is a no-op when that variable is absent.
The key is never written into the repo or the VM image.

## Checks

```sh
dev/nix-shell 'just agent-test'
dev/serena test
dev/serena smoke
```

The tests cover quoting, working directory, exit status, nested reuse, shell
selection, input changes, and the hook's scope and permission behavior.
The broker tests check launcher invalidation and graceful or forced shutdown.
The optional smoke check connects two clients and checks shared-process reuse,
the query-only tool list, rejected project switching, and stale-process handling.
It can start the shared server. Use `dev/serena stop` when no clients need it.
