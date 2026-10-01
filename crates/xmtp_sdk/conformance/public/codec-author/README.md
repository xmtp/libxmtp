# Codec author package

This independent package depends only on the new SDK. `ReadingCodec` imports
`ContentCodec<T>`, `EncodedContent`, and `ContentTypeId` from the public package
root. It uses no binding, primitives, proto package, or legacy adapter.

Run from the repository root, after SDK generation:

```sh
dev/nix-shell 'just sdk codec-author types'
dev/nix-shell 'just sdk codec-author node'
dev/nix-shell 'just sdk codec-author browser'
```

The staging script installs the package and SDK into separate Node and browser
consumer directories. For browser, it changes the SDK import to
`xmtp-sdk-browser`. The Node import is `xmtp-sdk`. Both use ESM.

The positive consumer checks encode, Group and Dm send, Group and Dm prepare,
and Message reply. The negative consumer passes a number to each call. Each
of its six calls must fail with a type error.

The runtime proof uses real public clients. It checks a custom codec round
trip, exact envelope fields, later minor versions, separate client registries,
nested replies, skipped policy hooks, and failed encode, fallback, and push
steps before publication. The external test runner reads this worktree's backend
and decodes its public wire schemas. It checks the published push flags and
proves that sends have no compression unless the caller requests it. These
checks require this worktree's Docker backend. Signer creation is in the external
test runner. The codec package has no signer-library dependency.

`XMTP_SDK_GENERATED_DIR` selects generated inputs. For final release checks,
`XMTP_SDK_PACKAGES_DIR` selects the staged `node` and `browser` package folders.
The script keeps those products' manifests, declarations, bundled dependencies,
and assets.
