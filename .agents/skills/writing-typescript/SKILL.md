---
name: writing-typescript
description: Use when writing or reviewing TypeScript or JavaScript in this repository; covers the pnpm task graph, local bindings, shared lint rules, and formatting.
---

# Writing TypeScript and JavaScript

The repository has one root pnpm workspace. Run `dev/nix-shell 'just install-js'`
once before package work. Package scripts use the root task graph. It does not
build native bindings. Use the relevant `just js`, `just cli`, `just web-chat`,
or `just docs` recipe when a task needs Node or WASM bindings; those recipes
stage the local bindings first. Use `dev/nix-shell '<command>'` for a focused
package command. Do not run `pnpm` outside the Nix shell.

Shared TypeScript configs are in `dev/js/`. Oxlint and Oxfmt configs stay at the
repository root for tool and editor discovery. Run checks through package
scripts or `just` recipes. The shared Oxlint config checks correctness,
supported type-checked rules, source aliases, type-only imports, and selected
type-safety rules. Test overrides allow loose mocks and fixtures, but still
check unused code and promise handling. Keep test exceptions in the shared
config instead of copying rule lists into packages.

JavaScript formatting is separate from `just lint-config`. Use
`dev/nix-shell 'just format-js'` to write formatting or
`dev/nix-shell 'just lint-js-format'` to check it. Read the relevant package
`AGENTS.md` for build order, test commands, and behavior rules.
