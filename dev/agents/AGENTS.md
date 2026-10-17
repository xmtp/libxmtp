# Agent tools

The tests of the wrapper, the hook, the command runner, and the Kache settings
live in `dev/tests` and run in `dev/nix-shell 'just test dev'`.
After changes to the navigation launcher, run
`dev/nix-shell 'just outline dev/agents/nix_hook.py'` and
`dev/nix-shell 'just show dev/agents/nix_hook.py rewrite'`.
Use `dev/nix-shell --shell rust 'uv lock --project dev/agents'` after dependency
changes. Keep ast-outline and its grammar dependencies pinned in `uv.lock`.
Run `dev/serena test` after changes to broker identity or shutdown.
Run `dev/serena stop` before testing a changed Serena launcher or configuration.
The launcher must keep one server per worktree. Do not expose edit tools or
`activate_project`. Keep dependencies pinned in `uv.lock`.
Test the MCP connection with two clients after changes to the server lifecycle.
Do not change global Codex settings or bypass user trust controls in setup code.
