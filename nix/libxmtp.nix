{ shells
, stdenv
, darwin
, lib
, fenix
, mkToolchain
, pkg-config
, mktemp
, jdk21
, kotlin
, diesel-cli
, gnuplot
, flamegraph
, cargo-flamegraph
, cargo-expand
, cargo-udeps
, inferno
, openssl
, sqlcipher
, sqlite
, corepack
, lnav
, zstd
, foundry-bin
, graphite-cli
, jq
, llvmPackages
, ...
}:

let
  inherit (stdenv) isDarwin;
  inherit (shells) combineShell;
  mkShell =
    top:
    (
      combineShell
        {
          otherShells = with shells;
            [
              mkLinters
              mkCargo
              mkRustWasm
              mkGrpc
            ];
          extraInputs = top;
        });
  rust-toolchain = mkToolchain [ "wasm32-unknown-unknown" "x86_64-unknown-linux-gnu" ] [ "clippy-preview" "rust-docs" "rustfmt-preview" "llvm-tools-preview" ];
  darwinAttrs = {
    # set the linker for macos
    CC_wasm32_unknown_unknown = "${llvmPackages.clang-unwrapped}/bin/clang";
    AR_wasm32_unknown_unknown = "${llvmPackages.clang-unwrapped}/bin/llvm-ar";
  };
in
mkShell ({
  OPENSSL_DIR = "${openssl.dev}";
  # LLVM_PATH = "${llvmPackages_19.stdenv}";
  # CXX_wasm32_unknown_unknown = "${llvmPackages_20.clang-unwrapped}/bin/clang++";
  # AS_wasm32_unknown_unknown = "${llvmPackages_20.clang-unwrapped}/bin/llvm-as";
  # STRIP_wasm32_unknown_unknown = "${llvmPackages_20.clang-unwrapped}/bin/llvm-strip";
  # disable -fzerocallusedregs in clang
  hardeningDisable = [ "zerocallusedregs" ];
  OPENSSL_LIB_DIR = "${lib.getLib openssl}/lib";
  OPENSSL_NO_VENDOR = 1;
  nativeBuildInputs = [ pkg-config ];
  buildInputs =
    [
      rust-toolchain
      fenix.rust-analyzer
      foundry-bin

      # native libs
      openssl
      sqlite
      sqlcipher
      zstd
      # emscripten

      mktemp
      jdk21
      kotlin
      diesel-cli
      graphite-cli

      # Random devtools
      # tokio-console
      gnuplot
      flamegraph
      cargo-flamegraph
      cargo-udeps
      cargo-expand
      inferno
      lnav
      jq

      # make sure to use nodePackages! or it will install yarn irrespective of environmental node.
      corepack
    ]
    ++ lib.optionals isDarwin [
      darwin.cctools
    ] ++ lib.optionals stdenv.isLinux [

    ];
} // lib.optionalAttrs isDarwin darwinAttrs)
