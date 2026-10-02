# Documentation

Before you submit a change in `docs/` or `apps/docs/`, you MUST run both checks
from the repository root:

```sh
dev/nix-shell 'just docs check'
dev/nix-shell 'just docs check-external'
```

`check` checks local source links in `apps/docs/src/content/docs` and the composed
site in `apps/docs/_site`. `check-external` uses Lychee from the locked docs Nix
shell to check HTTP and HTTPS URLs in that site. The external check is required
locally even though it no longer runs in docs CI. Report failures and fix broken
links before submission. Do not claim a full site pass from a fixture or dry run.

## Prepare the site

Install the locked JavaScript dependencies with
`dev/nix-shell 'just install-js'`. Use site and reference output that matches the
source changes you will submit.

To build the site locally, run `dev/nix-shell 'just docs build'`. Generate the Rust
reference with `dev/nix-shell 'dev/agent-run cargo doc --locked --no-deps'`, then
copy `target/doc/` to `apps/docs/generated/reference/rust/`. The site build writes
`apps/docs/dist/`, including the JavaScript references.

You can instead download matching artifacts from a successful **Build and Deploy
Docs** run. Use its numeric run ID in place of `<run-id>`:

```sh
dev/nix-shell 'gh run download <run-id> --name docs-site --dir apps/docs/dist'
dev/nix-shell 'gh run download <run-id> --name docs-rust --dir apps/docs/generated/reference/rust'
```

For the pull request setup, compose and check without Kotlin and Swift references:

```sh
dev/nix-shell 'DOCS_SKIP_NATIVE_REFERENCES=1 just docs compose'
dev/nix-shell 'DOCS_SKIP_NATIVE_REFERENCES=1 just docs check'
dev/nix-shell 'just docs check-external'
```

For the full push setup, also download `docs-kotlin` to
`apps/docs/generated/reference/kotlin/`. Download `docs-swift` to a temporary
directory, then extract its `docs-swift.tar` to
`apps/docs/generated/reference/swift/`. Run `dev/nix-shell 'just docs compose'`
and both checks without `DOCS_SKIP_NATIVE_REFERENCES`. Do not use an old composed
site after source changes.

## External check scope

The recipe keeps the removed CI step's HTTP and HTTPS schemes, two retries,
HTML patterns, and exclusions. It checks the site root, the guide sections, and
the Browser, Node, Agent, error glossary, and limits references. Rust, Kotlin, and
Swift generated HTML is outside this scan, as it was in CI.

The exclusions cover the two canonical docs domains, GitHub edit URLs, GitHub
source URLs with a full commit hash, the example collector URL, and the IANA
registry. Local links still use `check`. Run with network access. If GitHub needs
a token, supply `GITHUB_TOKEN` in the environment; do not put it in a command or
commit it.
