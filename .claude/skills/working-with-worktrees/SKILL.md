---
name: working-with-worktrees
description: Use when working in a git worktree, starting the local Docker stack, or debugging port and database conflicts between checkouts - covers per-worktree slots, the generated env file, and the commands that report and release them
---

# Working with worktrees in libxmtp

Each worktree gets its own Compose project and host ports, so several can run the
stack at once. Never assume port 5050: that is only the main checkout.

```sh
just backend status     # this checkout's project, slot, ports, and URLs
just backend release    # stop this worktree's stack and free its slot
```

`dev/worktree-env` resolves the checkout to a slot and writes `dev/docker/.env`,
which Compose reads and `dev/docker/load-env` sources for the recipes. A variable
already set in the environment always wins, so CI pointing a suite at a deployed
backend still works. Ports are `base + slot * 100`. The main checkout and any
plain clone (so CI) are slot 0, so their ports never move.

Read addresses from the environment, never a literal: `XMTP_BACKEND_URL` and
`DATABASE_URL` in scripts and SDK tests, `xmtp_configuration::backend_test_url()`
or `DockerUrls::anvil()` in Rust, `xmtp_common::toxiproxy()` for Toxiproxy (the
upstream `TOXIPROXY` static hardcodes its port). Ports in `docs/` and `AGENTS.md`
are slot-0 values.

`just backend up` refuses to start when another project holds one of this
worktree's ports, and names both. A stack from a deleted worktree keeps running:
find it with `docker ps` and stop it with `docker compose -p <project> down`.
