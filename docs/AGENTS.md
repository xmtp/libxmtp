# Documentation

Before you submit a docs change, run this check from the repository root:

```sh
dev/nix-shell 'just docs check'
```

This checks local source links in `apps/docs/src/content/docs` and the composed
site in `apps/docs/_site`. It does not check external URLs.

The check needs the locked JavaScript dependencies. If needed, install them with
`dev/nix-shell 'just install-js'`.

Use a current composed site. To create it, run
`dev/nix-shell 'just docs compose'`. Composition needs `apps/docs/dist` from the
docs build and the generated Rust, Kotlin, and Swift references in
`apps/docs/generated/reference`.

For a pull request artifact without Kotlin and Swift references, use
`DOCS_SKIP_NATIVE_REFERENCES=1` for both commands:

```sh
dev/nix-shell 'DOCS_SKIP_NATIVE_REFERENCES=1 just docs compose'
dev/nix-shell 'DOCS_SKIP_NATIVE_REFERENCES=1 just docs check'
```
