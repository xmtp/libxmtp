# Agent tools

Run `dev/nix-shell 'just agent-test'` after changes to the wrapper or hook.
Run `dev/serena test` after changes to broker identity or shutdown.
Run `dev/serena stop` before testing a changed Serena launcher or configuration.
The launcher must keep one server per worktree. Do not expose edit tools or
`activate_project`. Keep dependencies pinned in `uv.lock`.
Test the MCP connection with two clients after changes to the server lifecycle.
Do not change global Codex settings or bypass user trust controls in setup code.
