{ stdenv
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
, cargo-nextest
, cargo-deny
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
, curl
, buf
, protobuf
, protolint
, mkShell
, wasm-bindgen-cli_0_2_100
, wasm-pack
, binaryen
, emscripten
, ...
}:

let
  inherit (stdenv) isDarwin;
  rust-toolchain = mkToolchain [ "wasm32-unknown-unknown" "x86_64-unknown-linux-gnu" ] [ "clippy-preview" "rust-docs" "rustfmt-preview" "llvm-tools-preview" ];
  darwinAttrs = {
    # set the linker for macos
    # libcxx is needed to find standard library headers (like 'limits.h')
    CC_wasm32_unknown_unknown = "${llvmPackages.libcxxClang}/bin/clang";
    AR_wasm32_unknown_unknown = "${llvmPackages.bintools-unwrapped}/bin/llvm-ar";
  };
in
mkShell ({
  OPENSSL_DIR = "${openssl.dev}";
  # LLVM_PATH = "${llvmPackages_19.stdenv}";
  # CXX_wasm32_unknown_unknown = "${llvmPackages_20.clang-unwrapped}/bin/clang++";
  # AS_wasm32_unknown_unknown = "${llvmPackages_20.clang-unwrapped}/bin/llvm-as";
  # STRIP_wasm32_unknown_unknown = "${llvmPackages_20.clang-unwrapped}/bin/llvm-strip";
  # disable -fzerocallusedregs in clang
  hardeningDisable = [ "zerocallusedregs" "stackprotector" ];
  OPENSSL_LIB_DIR = "${lib.getLib openssl}/lib";
  OPENSSL_NO_VENDOR = 1;
  STACK_OVERFLOW_CHECK = 0;
  nativeBuildInputs = [ pkg-config wasm-pack wasm-bindgen-cli_0_2_100 binaryen emscripten ];
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
      cargo-deny
      cargo-flamegraph
      cargo-nextest
      inferno
      lnav
      jq
      curl

      # Protobuf
      buf
      protobuf
      protolint

      # yarn/nodejs
      corepack
    ]
    ++ lib.optionals isDarwin [
      darwin.cctools
    ] ++ lib.optionals stdenv.isLinux [

    ];
} // lib.optionalAttrs isDarwin darwinAttrs)
