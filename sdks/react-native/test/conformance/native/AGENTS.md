# Native callback marshalling regression

Run from the repository root through `dev/nix-shell`:

```sh
dev/nix-shell 'python3.11 sdks/react-native/test/conformance/native/run.py --generated-cpp <generated-xmtp_sdk.cpp> --fork <pinned-fork-checkout> --hermes-source <hermes-source> --hermes-build <hermes-build> --sdk-library <libxmtp_sdk.dylib>'
```

The runner currently targets Darwin arm64 and enables AddressSanitizer. Supply the
matched generated wrapper, fork runtime, Hermes build, and native SDK library.
It uses real Hermes and Rust buffers, with C++ completion counters. Do not report
it as proof of Rust oneshot ownership, ReactHost teardown, or OS lifecycle.
Those gates require the installed consumer journeys and actual Rust future tests.
