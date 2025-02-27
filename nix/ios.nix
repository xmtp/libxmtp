# This build will only work on darwin
{ stdenv
, darwin
, lib
, fenix
, pkg-config
, mkShell
, openssl
, sqlite
, zstd
, llvmPackages_19
, xcbuild
, ...
}:

let
  inherit (stdenv) isDarwin;
  inherit (darwin.apple_sdk) frameworks;

  iosTargets = [
    "x86_64-apple-darwin"
    "aarch64-apple-ios"
    "x86_64-apple-ios"
    "aarch64-apple-ios-sim"
  ];

  # Pinned Rust Version
  rust-toolchain = fenix.combine [
    fenix.stable.cargo
    fenix.stable.rustc
    (lib.forEach
      iosTargets
      (target: fenix.targets."${target}".stable.rust-std))
  ];
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

      # native libs
      zstd
      openssl
      sqlite
      xcbuild
    ]
    ++ lib.optionals isDarwin [
      frameworks.CoreServices
      frameworks.Carbon
      frameworks.ApplicationServices
      frameworks.AppKit
      darwin.cctools
    ];
}
