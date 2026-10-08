{
  lib,
  mkShell,
  xmtp,
  emscripten,
  wasm-pack,
  binaryen,
  llvmPackages,
  wasm-bindgen-cli,
  sqlite,
  cargo-nextest,
  chromedriver,
  xmtp-pnpm,
  nodejs_26,
  stdenv,
  google-chrome,
  chromium,
  kache,
}:
mkShell (
  xmtp.shellCommon.wasmEnv
  // {
    inputsFrom = [ xmtp.base.commonArgs ];
    # A nested WASM shell must use the Nix compiler with the Nix host SDK.
    shellHook =
      xmtp.shellCommon.compilerCacheHook
      + lib.optionalString stdenv.isDarwin ''
        export CC_aarch64_apple_darwin="${stdenv.cc}/bin/cc"
        export CXX_aarch64_apple_darwin="${stdenv.cc}/bin/c++"
        export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER="${stdenv.cc}/bin/cc"
      '';
    nativeBuildInputs = [
      emscripten
      wasm-pack
      binaryen
      llvmPackages.lld
      wasm-bindgen-cli
    ];
    buildInputs = [
      kache
      (xmtp.mkNativeToolchain
        [ "wasm32-unknown-unknown" ]
        [
          "clippy-preview"
          "rustfmt-preview"
        ]
      )
      sqlite
      cargo-nextest
      chromedriver
      xmtp-pnpm
      nodejs_26
    ]
    ++ lib.optionals stdenv.isDarwin [ google-chrome ]
    ++ lib.optionals stdenv.isLinux [ chromium ];
    SQLITE = "${sqlite.dev}";
    SQLITE_OUT = "${sqlite.out}";
    CARGO_PROFILE_TEST_DEBUG = 0;
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    XMTP_NIX_ENV = 1;
  }
)
