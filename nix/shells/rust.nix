# Focused Rust dev shell for crates/ and bindings/ work.
# Supports: dev/lint, dev/lint-rust, cargo test, cargo nextest, WASM checks.
# Does NOT include debugging/profiling tools or convenience packages — see local.nix.
{
  stdenv,
  darwin,
  lib,
  mkShell,
  foundry-bin,
  just,
  sqlcipher,
  xmtp-pnpm,
  rust-analyzer,
  python311,
  uv,
  xmtp,
}:
let
  inherit (stdenv) isDarwin;
  inherit (xmtp) shellCommon;
  rust-toolchain =
    xmtp.mkNativeToolchain
      [ "wasm32-unknown-unknown" "x86_64-unknown-linux-gnu" ]
      [ "rust-src" "clippy-preview" "rust-docs" "rustfmt-preview" "llvm-tools-preview" ];
in
mkShell {
  meta.description = "Rust development environment for libXMTP crates and bindings";

  XMTP_DEV_SHELL = "rust";

  # A nested Rust shell must not keep the local/iOS shell's Xcode overrides.
  # Use Nix's compiler wrapper with Nix's SDK and library search paths.
  shellHook = lib.optionalString isDarwin ''
    export CC_aarch64_apple_darwin="${stdenv.cc}/bin/cc"
    export CXX_aarch64_apple_darwin="${stdenv.cc}/bin/c++"
    export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER="${stdenv.cc}/bin/cc"
  '';

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
      # .envrc auto-loads this shell, so the repo's `just` workflow must resolve here
      just
      sqlcipher
      xmtp-pnpm
      rust-analyzer
      python311
      uv
    ]
    ++ shellCommon.wasmTools
    ++ shellCommon.cargoTools
    ++ shellCommon.agentTools
    ++ shellCommon.cargoCiTools
    ++ shellCommon.protoTools
    ++ shellCommon.lintTools
    ++ lib.optionals isDarwin [
      darwin.cctools
    ];
}
