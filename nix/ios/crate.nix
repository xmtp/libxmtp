{ openssl
, sqlite
, perl
, binutils
, lib
, pkg-config
, stdenv
, craneLib
, rustTarget
, darwin
}:
let
  inherit (darwin.apple_sdk) frameworks;
  inherit (stdenv.buildPlatform) isDarwin;
  inherit (craneLib.fileset) commonCargoSources;
  upperTarget = lib.strings.toUpper (builtins.replaceStrings [ "-" ] [ "_" ] rustTarget);
  root = ./../..;
  src = craneLib.cleanCargoSource root;

  # TARGET_CC = "${pkgsCross.stdenv.cc}/bin/${pkgsCross.stdenv.cc.targetPrefix}cc";
  # Common arguments can be set here to avoid repeating them later
  commonArgs = {
    inherit src;
    strictDeps = true;

    nativeBuildInputs = [
      pkg-config
      stdenv.cc
      perl
      binutils
    ];

    # Dependencies which need to be built for the platform on which
    # the binary will run. In this case, we need to compile openssl
    # so that it can be linked with our executable.
    buildInputs = [
      # Add additional build inputs here
      openssl
      sqlite
    ] ++ lib.optionals isDarwin [
      frameworks.Security
      frameworks.SystemConfiguration
    ];
  };

  linkerArgs = {
    # MAYBE INCORRECT DEPENDING ON MEANING OF HOST/TARGET in stdenv.cc/gcc
    HOST_CC = "${stdenv.cc.nativePrefix}cc";
    TARGET_CC = "${stdenv.cc.targetPrefix}cc";
    "${upperTarget}_OPENSSL_DIR" = "${openssl.out}";
    "${upperTarget}_OPENSSL_LIB_DIR" = "${openssl.out}/lib";
    "${upperTarget}_OPENSSL_INCLUDE_DIR" = "${openssl.dev}/include";

    CARGO_BUILD_TARGET = rustTarget;
    # See: https://doc.rust-lang.org/cargo/reference/config.html#target
    # "CARGO_TARGET_${upperTarget}_LINKER" = "${cross.stdenv.cc.targetPrefix}cc";
    "CARGO_TARGET_${upperTarget}_LINKER" = "${stdenv.cc.targetPrefix}cc";
  };

  cargoArtifacts = craneLib.buildDepsOnly (commonArgs // linkerArgs);

  fileSetForCrate = crate: lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      ./../../Cargo.toml
      ./../../Cargo.lock
      (commonCargoSources ./../../xmtp_api_grpc)
      (commonCargoSources ./../../xmtp_api_http)
      (commonCargoSources ./../../xmtp_cryptography)
      (commonCargoSources ./../../xmtp_id)
      (commonCargoSources ./../../xmtp_mls)
      (commonCargoSources ./../../xmtp_proto)
      (commonCargoSources ./../../xmtp_v2)
      (commonCargoSources ./../../xmtp_user_preferences)
      (commonCargoSources ./../../examples/cli)
      (commonCargoSources ./../../mls_validation_service)
      (commonCargoSources ./../../bindings_node)
      (commonCargoSources ./../../bindings_wasm)
      (commonCargoSources ./../../xtask)
      ./../../xmtp_id/src/scw_verifier/chain_urls_default.json
      ./../../xmtp_mls/migrations
      crate
    ];
  };

  crateArgs = (commonArgs // linkerArgs) // {
    inherit cargoArtifacts;
    inherit (craneLib.crateNameFromCargoToml {
      inherit src;
    }) version;
    # NB: we disable tests since we'll run them all via cargo-nextest
    doCheck = false;
  };

  xmtpv3 = craneLib.buildPackage (crateArgs // {
    pname = "xmtpv3";
    cargoExtraArgs = "-p xmtpv3";

    RUST_BACKTRACE = 1;
    src = fileSetForCrate ./../../bindings_ffi;
  });
in
xmtpv3
