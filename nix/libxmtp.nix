{ shells
, stdenv
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
, inferno
, openssl
, sqlcipher
, sqlite
, corepack
, lnav
, zstd
, llvmPackages_19
, wasm-bindgen-cli
, ...
}:

let
  inherit (stdenv) isDarwin;
  inherit (darwin.apple_sdk) frameworks;
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

  rust-toolchain = fenix.fromToolchainFile {
    file = ./../rust-toolchain.toml;
    sha256 = "sha256-AJ6LX/Q/Er9kS15bn9iflkUwcgYqRQxiOIL2ToVAXaU=";
  };
in
mkShell {
  OPENSSL_DIR = "${openssl.dev}";
  LLVM_PATH = "${llvmPackages_19.stdenv}";
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
      rust-toolchain
      wasm-bindgen-cli
      fenix.rust-analyzer
      zstd

      # native libs
      openssl
      sqlite
      sqlcipher

      mktemp
      jdk21
      kotlin
      diesel-cli

      # Random devtools
      # tokio-console
      gnuplot
      flamegraph
      cargo-flamegraph
      inferno
      lnav

      # make sure to use nodePackages! or it will install yarn irrespective of environmental node.
      corepack
    ]
    ++ lib.optionals isDarwin [
      frameworks.CoreServices
      frameworks.Carbon
      frameworks.ApplicationServices
      frameworks.AppKit
      darwin.cctools
    ];
}
