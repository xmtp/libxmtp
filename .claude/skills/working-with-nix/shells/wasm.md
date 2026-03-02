# WASM Shell (`nix develop .#wasm`)

For WebAssembly builds and testing. Uses `fenix.stable` Rust (not the project-pinned 1.92.0).

**Source:** `nix/package/wasm.nix` (the `devShell` attribute)

## Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `CARGO_BUILD_TARGET` | `wasm32-unknown-unknown` | Default build target |
| `XMTP_NIX_ENV` | `1` | Nix environment is active |
| `CC_wasm32_unknown_unknown` | clang path | WASM C compiler |
| `AR_wasm32_unknown_unknown` | llvm-ar path | WASM archiver |
| `CFLAGS_wasm32_unknown_unknown` | clang include path | WASM C compiler flags |
| `SQLITE` | SQLite dev path | SQLite headers |
| `SQLITE_OUT` | SQLite out path | SQLite binaries |
| `CHROMEDRIVER` | ChromeDriver path | Chrome WebDriver |
| `WASM_BINDGEN_TEST_TIMEOUT` | `1024` | Test timeout seconds |
| `WASM_BINDGEN_TEST_ONLY_WEB` | `1` | Web-only tests |
| `RSTEST_TIMEOUT` | `90` | rstest timeout |
| `CARGO_PROFILE_TEST_DEBUG` | `0` | Disable debug in tests |
| `WASM_BINDGEN_TEST_WEBDRIVER_JSON` | Config path | WebDriver config |
| `NIX_DEBUG` | `1` | Nix debug output |

## Rust Configuration

Uses `fenix.stable` toolchain (not the project-pinned version) with:
- `cargo`, `rustc`
- `wasm32-unknown-unknown` target

## Tools Included

**Build (from commonArgs nativeBuildInputs):**
`wasm-pack`, `wasm-bindgen-cli`, `binaryen`, `emscripten`, `llvmPackages.lld`

**Testing:**
`google-chrome`, `chromedriver`, `cargo-nextest`

**Other:**
`corepack`

## Building WASM Package

```bash
# In the WASM shell
wasm-pack build --target web bindings/wasm

# Or build as Nix package
nix build .#wasm-bindings
```
