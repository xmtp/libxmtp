---
name: working-with-worktrees
description: Use when creating a git worktree, running the Docker stack or tests from more than one checkout at once, or debugging port and database conflicts between checkouts - covers per-worktree slots, the generated env file, shared caches, the shared stash, and the commands that report and release a stack
---

# Working with worktrees in libxmtp

Each worktree gets its own Compose project and host ports, so several can run
the stack at once. Never assume port 5050: that is only the main checkout.
Every port in `docs/` and `AGENTS.md` is a slot-0 value.

## Start work in a new worktree

```sh
git worktree add ../<name>/libxmtp -b <branch> origin/self-hosted
cd ../<name>/libxmtp
dev/nix-shell 'just backend up'      # this worktree's own stack
dev/nix-shell 'just backend status'  # project, slot, ports, URLs
```

`dev/worktree-env` claims a slot for the checkout and writes `dev/docker/.env`
(gitignored). Ports are `base + slot * 100`. The main checkout and any plain
clone, so CI, are slot 0.

## Run in parallel without conflicts

- **Never pass a port by hand.** Every `just` recipe that needs the stack
  (`just test`, `just backend *`, the SDK test recipes) sources
  `dev/docker/.env` first. A variable already set in your environment wins, so
  set `XMTP_BACKEND_URL` or `DATABASE_URL` only to point at a different backend.
- **Read addresses from the environment in code.** Rust:
  `xmtp_configuration::backend_test_url()`, `backend_test_toxic_url()`,
  `DockerUrls::anvil()`; Toxiproxy: `xmtp_common::toxiproxy()` (the upstream
  `TOXIPROXY` static hardcodes its port). Scripts and SDK tests:
  `XMTP_BACKEND_URL`, `DATABASE_URL`.
- **Share the compile cache.** Each worktree has its own `target/`. When
  several build at once, `source dev/sccache-env` in each Nix shell. It unsets
  `CARGO_INCREMENTAL` to keep Cargo's local incremental defaults and caches
  eligible non-incremental dependencies. The default cache cap is 10 GiB.
  A running sccache server keeps its cache settings; check `just cache-stats`.
  Do not share a mutable `target/` directory across worktrees. See
  `docs/nix-setup.md` for cache limits and how to disable the wrapper.
- **The stash stack is shared** across worktrees. Never bare `git stash` /
  `git stash pop`; another session can pop your entry. Park work in a WIP
  commit instead.
- **Flakes see only tracked files.** `git add` a new file in the worktree
  before `nix build` or `just backend build` can see it.

## Fix a conflict

`just backend up` refuses to start when another project holds one of this
worktree's ports, and names both:

```text
worktree-env: port 5150 is held by Compose project 'libxmtp-old-branch-1'.
worktree-env: this worktree is 'libxmtp-agent-efficiency-1' (slot 1). Stop the other stack, or set XMTP_WORKTREE_SLOT to a free slot.
```

- A stack from a deleted worktree keeps running. Find it with `docker ps` and
  stop it with `docker compose -p <project> down`.
- A port held by a non-Compose process is an unrelated local service. Stop it
  or override with `XMTP_WORKTREE_SLOT=<n>`.
- All 40 slots claimed: run `just backend release` in worktrees you are done
  with. Claims for deleted worktrees are pruned on the next run.

## Finish

```sh
dev/nix-shell 'just backend release'   # stop the stack, free the slot
git worktree remove ../<name>/libxmtp
```
