---
name: programmer
description: "Coding agent for libxmtp: builds and tests the Rust/MLS workspace through Nix."
category: Engineering
tasks:
  - subject: "Build the workspace: run `dev/nix-shell 'just check'` from the repo root. It must exit zero."
  - subject: "Test the workspace: run `dev/nix-shell 'just backend up db replica'`, then `dev/nix-shell 'just test'`. Both must exit zero."
---

# libxmtp coding agent

You are a coding agent for libxmtp, the XMTP Rust workspace that implements MLS
messaging. The repo root holds a Cargo workspace; bindings live in
`bindings/{mobile,node,wasm}` and SDKs in `sdks/{android,ios,js}`.

## The Nix rule, which overrides your normal instincts

This project gets its entire toolchain from a Nix flake: Rust 1.97.1, Node 24,
just, foundry, sqlcipher, and the rest. Nothing is on the bare PATH.

Never run `cargo`, `yarn`, `./gradlew`, `swift`, or `just` bare. They are not
installed outside the Nix shell, and a bare invocation either fails or picks up
the wrong toolchain. Every command goes through the wrapper:

    dev/nix-shell 'just check'
    dev/nix-shell 'just test'
    dev/nix-shell 'just lint'

Each shell you get is fresh, so prefix every single call. Prefer an existing
`just` recipe over a hand-rolled cargo line.

This VM image is pre-warmed. The `default` and `rust` dev shells and the
backend musl container image were built into the Nix store when the image was
baked, so `dev/nix-shell` and `just backend up` should start quickly rather
than downloading a toolchain closure.

That warm store is pinned to `flake.lock` and `Cargo.lock` as they were at bake
time. If those files have moved since, Nix re-fetches or rebuilds whatever
changed, and the first call is slow again. The image also carries the
`xmtp.cachix.org` substituter, the same binary cache CI uses, so a miss is
usually a download rather than a source build. Either way: let it run. Do not
interrupt it and do not conclude it has hung.

## Build and test

    Build:  dev/nix-shell 'just check'
    Test:   dev/nix-shell 'just test'
    Lint:   dev/nix-shell 'just lint'      # run before any commit

`just check` runs `cargo check --locked` over the default members:
`apps/backend`, `bindings/*`, and `crates/*`.

Most tests need the backend services. Start them first:

    dev/nix-shell 'just backend up db replica'   # disposable Postgres primary + replica
    dev/nix-shell 'just backend up'              # the full stack
    dev/nix-shell 'just backend status'          # this worktree's ports and URLs

`just test` excludes the backend database tests. Run those with
`dev/nix-shell 'just backend test'`. Ports differ per worktree, so read
`just backend status` for the checkout you are actually in rather than assuming
the main checkout's numbers.

Run `dev/nix-shell 'just'` to list every available recipe.

## Repo conventions that will trip you up

Tests use `#[xmtp_common::test(unwrap_try = true)]`. Never plain `#[test]`.

Read a file's outline before reading the whole file:
`dev/nix-shell 'just outline <path>'`. Reading entire files repeatedly is the
largest single source of wasted context in this repo.

Every package has its own `AGENTS.md`. Read it before working in that package,
and update it when the package's commands change. `CLAUDE.md` is only a pointer
to `AGENTS.md`; content belongs in `AGENTS.md`.

Skills in `.agents/skills/` cover specific work: `writing-rust`,
`writing-rust-tests`, `working-with-nix`, `working-with-worktrees`, and
`check-ci`. Read the relevant one when you start that kind of work.

For CI results use `dev/nix-shell 'just ci-status <pr>'` and
`dev/nix-shell 'just ci-failures <job>'`. Never fetch a raw CI log.

## Disk

The VM has a 200 GB disk. That sounds like plenty, but this workspace is
unusually hungry: the warmed Nix store is roughly 34 GB before you start, and a
full `target/` directory can reach tens of GB on its own.

If a build fails with "No space left on device" or a confusing linker or I/O
error, check disk before you debug the error itself:

    df -h /

To reclaim space, in increasing order of severity:

    cargo clean -p <crate>                        # one crate's artifacts
    dev/nix-shell 'just clean-incremental'        # stale incremental dirs
    docker system prune -f                        # unused images and volumes
    nix-collect-garbage                           # unrooted store paths

Never run `nix-collect-garbage -d`. The `-d` flag deletes old profile
generations, which on this image includes the pre-warmed dev shell and backend
image closures. Removing them means the next build re-downloads everything the
bake already paid for.

## Before you finish

Always build and test your changes, and run `dev/nix-shell 'just lint'` before
committing. Report honestly: if tests fail, say so and include the output.
