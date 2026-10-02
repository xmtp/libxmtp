{
  lib,
  mkShell,
  sqlite,
  xmtp,
  chromedriver,
  google-chrome,
  chromium,
  xmtp-pnpm,
  nodejs_26,
  cargo-nextest,
  stdenv,
  callPackage,
}:
let
  inherit (xmtp) base;
  # Pinned Rust Version (must use mkToolchain to match the rest of the project)
  rust-toolchain =
    xmtp.mkNativeToolchain
      [ "wasm32-unknown-unknown" ]
      [ "clippy-preview" "rustfmt-preview" ];

  # The browser product uses the same matched generated roots as all SDKs.
  bin = (callPackage ./xmtp-sdk.nix { }).generated;
  commonArgs = base.commonArgs;
  commonEnv = {
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    inherit (xmtp.shellCommon.wasmEnv)
      CC_wasm32_unknown_unknown
      AR_wasm32_unknown_unknown
      CFLAGS_wasm32_unknown_unknown
      ;
  };

  devShell = mkShell (
    commonEnv
    // {
      inputsFrom = [ commonArgs ];
      buildInputs = [
        rust-toolchain
        cargo-nextest
        chromedriver
        xmtp-pnpm
        nodejs_26
      ]
      # chromium unsupported on darwin
      # google-chrome unsupported on aarch64-linux
      # Firefox compiles from scratch on everything but x86_64 (unreliable build)
      ++ lib.optionals stdenv.isDarwin [ google-chrome ]
      ++ lib.optionals stdenv.isLinux [ chromium ];
      inherit (xmtp.shellCommon.wasmEnv)
        RSTEST_TIMEOUT
        WASM_BINDGEN_TEST_TIMEOUT
        WASM_BINDGEN_TEST_WEBDRIVER_JSON
        CHROMEDRIVER
        ;

      SQLITE = "${sqlite.dev}";
      SQLITE_OUT = "${sqlite.out}";
      CARGO_PROFILE_TEST_DEBUG = 0;
      CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
      XMTP_NIX_ENV = 1;
    }
  );
in
{
  inherit devShell bin;
}
