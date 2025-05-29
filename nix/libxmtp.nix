{ stdenv
, darwin
, lib
, fenix
, pkg-config
, mktemp
, jdk21
, kotlin
, diesel-cli
, gnuplot
, flamegraph
, cargo-flamegraph
, cargo-udeps
, cargo-hakari
, inferno
, openssl
, sqlcipher
, sqlite
, corepack
, lnav
, zstd
, wasm-bindgen-cli
, foundry-bin
, graphite-cli
, xmtp
, ...
}:

let
  inherit (stdenv) isDarwin;
  mkShell =
    top:
    (
      xmtp.combineShell
        {
          otherShells = with xmtp.shells;
            [
              mkLinters
              mkCargo
              mkRustWasm
              mkGrpc
            ];
          extraInputs = top;
        });
  rust-toolchain = mkToolchain [ "wasm32-unknown-unknown" "x86_64-unknown-linux-gnu" ] [ "clippy-preview" "rust-docs" "rustfmt-preview" "llvm-tools-preview" ];
in
mkShell {
  OPENSSL_DIR = "${openssl.dev}";
  # LLVM_PATH = "${llvmPackages_19.stdenv}";
  # CC_wasm32_unknown_unknown = "${llvmPackages_20.clang-unwrapped}/bin/clang";
  # CXX_wasm32_unknown_unknown = "${llvmPackages_20.clang-unwrapped}/bin/clang++";
  # AS_wasm32_unknown_unknown = "${llvmPackages_20.clang-unwrapped}/bin/llvm-as";
  # AR_wasm32_unknown_unknown = "${llvmPackages_20.clang-unwrapped}/bin/llvm-ar";
  # STRIP_wasm32_unknown_unknown = "${llvmPackages_20.clang-unwrapped}/bin/llvm-strip";
  # disable -fzerocallusedregs in clang
  hardeningDisable = [ "zerocallusedregs" ];
  OPENSSL_LIB_DIR = "${lib.getLib openssl}/lib";
  OPENSSL_NO_VENDOR = 1;
  nativeBuildInputs = [ pkg-config ];
  buildInputs =
    [
      wasm-bindgen-cli
      rust-toolchain
      fenix.rust-analyzer
      zstd
      foundry-bin

      # native libs
      openssl
      sqlite
      sqlcipher

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
      cargo-hakari
      cargo-udeps
      inferno
      lnav

      # make sure to use nodePackages! or it will install yarn irrespective of environmental node.
      corepack
    ]
    ++ lib.optionals isDarwin [
      darwin.cctools
    ] ++ lib.optionals stdenv.isLinux [

    ];
}
