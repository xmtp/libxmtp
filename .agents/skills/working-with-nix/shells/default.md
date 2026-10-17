# Default Shell (`dev/nix-shell '...'`)

The local development shell for humans, agents, editors, and direnv. It is the
`rust` core (through `inputsFrom` and `rustShellEnv`) plus local tools. `ios`
is an alias. It has no Android SDK: Android work uses the `android` shell.

**Source:** `nix/shells/local.nix`

## Environment

Everything in [the rust shell](rust.md), with `XMTP_DEV_SHELL=local`, plus:

| Variable | Value |
| --- | --- |
| `XMTP_SDK_OXFMT` | nixpkgs `oxfmt`, which the cargo SDK render uses |
| `WASM_BINDGEN_TEST_TIMEOUT`, `RSTEST_TIMEOUT` | WASM test timeouts |
| `WASM_BINDGEN_TEST_WEBDRIVER_JSON`, `CHROMEDRIVER` | Nix Chrome for WASM tests |

On Darwin the rust shellHook sets the Xcode compiler environment for the host
and the Apple slices (see [the rust shell](rust.md)); this shell adds a
warning when Xcode is older than 16.

## Tools added to the core

rust-analyzer, watchexec, GNU sed, JDK 21 and `kotlin-language-server` for
editors, swiftformat (and swiftlint on Darwin), diesel-cli, the debug and
profiling tools (lldb, flamegraph, inferno; rr on Linux), gh, jq, curl,
git-cliff, and toxiproxy.
