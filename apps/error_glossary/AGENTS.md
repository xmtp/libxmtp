# error_glossary

Generate `docs/error_glossary.md` from public `ErrorCode` types in `crates/` and
`bindings/`. Keep the output order stable when types have the same name.

## Commands

Run from the repository root:

```sh
dev/nix-shell 'dev/gen-error-glossary'
dev/nix-shell 'cargo clippy -p error_glossary --no-deps -- -D warnings'
```

This package is not a workspace default member. Run its checks explicitly.
