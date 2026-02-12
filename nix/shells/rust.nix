# Focused Rust dev shell for crates/ and bindings/ work.
# Supports: dev/lint, dev/lint-rust, cargo test, cargo nextest, WASM checks.
# Does NOT include debugging/profiling tools or convenience packages — see local.nix.
{
  stdenv,
  darwin,
  lib,
  mkShell,
  foundry-bin,
  sqlcipher,
  corepack,
  xmtp,
}:
let
  inherit (stdenv) isDarwin;
  inherit (xmtp) shellCommon;
  rust-toolchain =
    xmtp.mkToolchain
      [ "wasm32-unknown-unknown" "x86_64-unknown-linux-gnu" ]
      [ "rust-src" "clippy-preview" "rust-docs" "rustfmt-preview" "llvm-tools-preview" ];
in
mkShell {
  meta.description = "Rust development environment for libXMTP crates and bindings";

  inherit (shellCommon.rustBase) hardeningDisable nativeBuildInputs LD_LIBRARY_PATH;
  inherit (shellCommon.rustBase.env)
    OPENSSL_DIR
    OPENSSL_LIB_DIR
    OPENSSL_NO_VENDOR
    STACK_OVERFLOW_CHECK
    XMTP_NIX_ENV
    ;
  inherit (shellCommon.wasmEnv)
    CC_wasm32_unknown_unknown
    AR_wasm32_unknown_unknown
    CFLAGS_wasm32_unknown_unknown
    ;

  buildInputs =
    shellCommon.rustBase.buildInputs
    ++ [
      rust-toolchain
      foundry-bin
      sqlcipher
      corepack
    ]
    ++ shellCommon.wasmTools
    ++ shellCommon.cargoTools
    ++ shellCommon.cargoCiTools
    ++ shellCommon.protoTools
    ++ shellCommon.lintTools
    ++ lib.optionals isDarwin [
      darwin.cctools
    ];
}
