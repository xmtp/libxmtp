# UniFFI core patch

Source: the published `uniffi_core` 0.32.2 crate from crates.io.
Upstream repository: <https://github.com/mozilla/uniffi-rs>.
Upstream commit: `7bc621ee9d0d15371d792612707ba5ed4111f950`.
Registry checksum: `f7530ae8efeaaa488622865966763fe0294832779fff80508c36d69c6a874a6a`.
License: MPL-2.0. The upstream LICENSE and source notices are included.

The source and normalized Cargo.toml are from the published crate. This copy
omits registry bookkeeping, the crate-local lockfile, release configuration,
and the original unnormalized Cargo.toml. One trailing space in the upstream
README is removed.

The only Rust change is in `src/ffi_converter_impls.rs`. The single-threaded
WASM `LowerReturn` implementation for `Result` now preserves typed argument
lift failures, as the native implementation does. It requires `Send + Sync`
for the error type so `anyhow::Error::downcast` can recover that type.
Return values and futures retain their existing WASM bounds. All exported SDK
error types satisfy the new bounds. The foreign ABI and metadata are unchanged.

The SDK bridge and pure-codec conformance tests check malformed IDs, nested
IDs, and non-ID errors. The regression proof also changes the Rust diagnostic
message to confirm that code, category, and retryability do not depend on text.
Remove this patch when the pinned upstream version supplies this behavior.
