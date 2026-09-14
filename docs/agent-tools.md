# Codex agent tools

The project configuration in `.codex/config.toml` provides a Nix shell hook and
read-only Rust tools through Serena. Start Codex from this worktree. Review and
trust the project and its hook through the normal Codex trust flow. Restart an
existing session to load the configuration. The hook needs host `python3` to
enter Nix. The Serena runtime uses the Python and uv versions from Nix.

## Nix commands

The hook runs shell commands with `bash -c` through `dev/nix-shell`. Use Bash
syntax for hooked commands. It does not add `errexit` or `nounset` to them.
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
