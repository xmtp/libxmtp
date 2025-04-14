{ stdenv
, darwin
, lib
, mkToolchain
, cargo-nextest
, pkg-config
, mktemp
, openssl
, sqlcipher
, sqlite
, zstd
, llvmPackages_19
, wasm-bindgen-cli
, foundry-bin
, mkShell
, ...
}:

let
  inherit (stdenv) isDarwin;
  inherit (darwin.apple_sdk) frameworks;
  rust-toolchain = mkToolchain [ "wasm32-unknown-unknown" ] [ ];
in
mkShell {
  OPENSSL_DIR = "${openssl.dev}";
  LLVM_PATH = "${llvmPackages_19.stdenv}";
  hardeningDisable = [ "zerocallusedregs" ];
  OPENSSL_LIB_DIR = "${lib.getLib openssl}/lib";
  OPENSSL_NO_VENDOR = 1;

  nativeBuildInputs = [ pkg-config ];
  buildInputs =
    [
      wasm-bindgen-cli
      rust-toolchain
      foundry-bin
      cargo-nextest

      # native libs
      zstd
      openssl
      sqlite
      sqlcipher

      mktemp # scripts
    ]
    ++ lib.optionals isDarwin [
      frameworks.CoreServices
      frameworks.Carbon
      darwin.cctools
    ];
}
