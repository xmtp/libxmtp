{ openssl
, sqlite
, binutils
, perl
, libiconv
, lib
, pkg-config
, stdenv
, system
, craneLib
, rustTarget
, pkgsCross
}:
let
  inherit (stdenv.buildPlatform) isDarwin;
  upperTarget = lib.strings.toUpper (builtins.replaceStrings ["-"] ["_"] rustTarget);
  src = craneLib.cleanCargoSource ./../..;
  # Common arguments can be set here to avoid repeating them later
  commonArgs = rec {
    inherit src;
    strictDeps = true;

    nativeBuildInputs = [
      pkg-config
      stdenv.cc
    ] ++ lib.optionals isDarwin [
      libiconv
    ];

    # Dependencies which need to be built for the platform on which
    # the binary will run. In this case, we need to compile openssl
    # so that it can be linked with our executable.
    buildInputs = [
      # Add additional build inputs here
      openssl
      sqlite
      binutils
      perl
    ] ++ lib.optionals isDarwin [
      libiconv
    ];

    HOST_CC = "${stdenv.cc.nativePrefix}cc";
    TARGET_CC = "${pkgsCross.stdenv.cc}/bin/${pkgsCross.stdenv.cc.targetPrefix}cc";

    # OPENSSL_STATIC = "1";
    # OPENSSL_DIR = "${pkgsCross.openssl.out}";
    OPENSSL_LIB_DIR = "${pkgsCross.openssl.out}/lib";
    OPENSSL_INCLUDE_DIR = "${pkgsCross.openssl.dev}/include";

    CARGO_BUILD_TARGET = rustTarget;
    CARGO_BUILD_RUSTFLAGS = [
      # "-C" "target-feature=+crt-static"

      # -latomic is required to build openssl-sys for armv6l-linux, but
      # it doesn't seem to hurt any other builds.
      # "-C" "link-args=-static -latomic"

      # https://github.com/rust-lang/cargo/issues/4133
      "-C" "linker=${TARGET_CC}"
    ];
  };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
  fileSetForCrate = lib.fileset.toSource {
    root = ./../..;
    fileset = lib.fileset.unions [
      ./../../Cargo.toml
      ./../../Cargo.lock
      ./../../bindings_ffi
      ./../../xmtp_api_grpc
      ./../../xmtp_cryptography
      ./../../xmtp_id
      ./../../xmtp_mls
      ./../../xmtp_proto
      ./../../xmtp_v2
      ./../../xmtp_user_preferences
    ];
  };
in craneLib.buildPackage (commonArgs // rec {
  inherit cargoArtifacts;
  inherit (craneLib.crateNameFromCargoToml {
      inherit src;
  }) version;
  inherit (craneLib.crateNameFromCargoToml {
      inherit src;
      cargoToml = ./../../bindings_ffi/Cargo.toml;
  }) pname;
  cargoExtraArgs = "-p xmtpv3";

  RUST_BACKTRACE=1;
  # See: https://doc.rust-lang.org/cargo/reference/config.html#target
  "CARGO_TARGET_${upperTarget}_LINKER" = "${pkgsCross.stdenv.cc.targetPrefix}cc";
  src = fileSetForCrate;
  # NB: we disable tests since we'll run them all via cargo-nextest
  doCheck = false;
})
