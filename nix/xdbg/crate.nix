{ openssl
, sqlite
, perl
, libiconv
, lib
, pkg-config
, stdenv
, craneLib
, darwin
, filesets
, mkShell
, rustTarget ? null
, isStatic ? false
}:
let
  inherit (stdenv) hostPlatform;
  upperTarget = lib.strings.toUpper (builtins.replaceStrings [ "-" ] [ "_" ] rustTarget);

  filesetForWorkspace = lib.fileset.toSource {
    root = ./../..;
    fileset = filesets.fileSetForWorkspace;
  };

  linkerArgs = if rustTarget == null then { } else {
    HOST_CC = "${stdenv.cc.nativePrefix}cc";
    TARGET_CC = "${stdenv.cc.targetPrefix}cc";
    "${upperTarget}_OPENSSL_DIR" = "${openssl.out}";
    "${upperTarget}_OPENSSL_LIB_DIR" = "${openssl.out}/lib";
    "${upperTarget}_OPENSSL_INCLUDE_DIR" = "${openssl.dev}/include";

    CARGO_BUILD_TARGET = rustTarget;
    "CARGO_TARGET_${upperTarget}_LINKER" = "${stdenv.cc.targetPrefix}cc";
    CARGO_BUILD_RUSTFLAGS = if isStatic then "-C target-feature=+crt-static" else "";
  };

  commonArgs = ({
    src = filesetForWorkspace;
    strictDeps = true;

    # Used to build on the current/build machine
    nativeBuildInputs = [
      pkg-config
      stdenv.cc
      perl
    ] ++ lib.optionals stdenv.isDarwin [
      libiconv
    ];

    # Libraries that will run on the host machine
    # that to be linked
    buildInputs = [
      # Add additional build inputs here
      openssl
      sqlite
    ] ++ lib.optionals hostPlatform.isDarwin
      (with darwin.apple_sdk;
      [
        libiconv
        frameworks.Security
        frameworks.SystemConfiguration
      ]);
    cargoExtraArgs = "--workspace --exclude xmtpv3";
    RUSTFLAGS = [ "--cfg" "tracing_unstable" ];
    OPENSSL_DIR = "${openssl.out}";
    OPENSSL_INCLUDE_DIR = "${openssl.dev}/include";
  } // linkerArgs);

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  bin = craneLib.buildPackage ({
    inherit cargoArtifacts;
    inherit (craneLib.crateNameFromCargoToml {
      cargoToml = ./../../xmtp_debug/Cargo.toml;
    }) version pname;
    cargoExtraArgs = "--package xdbg";
    src = filesetForWorkspace;
    doCheck = false;

    RUST_BACKTRACE = 1;
  } // commonArgs);

  devShell = mkShell
    {
      inputsFrom = [ bin ];
      OPENSSL_DIR = "${openssl.out}";
      OPENSSL_LIB_DIR = "${openssl.out}/lib";
      OPENSSL_INCLUDE_DIR = "${openssl.dev}/include";
      HOST_CC = "${stdenv.cc.nativePrefix}cc";
      TARGET_CC = "${stdenv.cc.targetPrefix}cc";
      "${upperTarget}_OPENSSL_DIR" = "${openssl.out}";
      "${upperTarget}_OPENSSL_LIB_DIR" = "${openssl.out}/lib";
      "${upperTarget}_OPENSSL_INCLUDE_DIR" = "${openssl.dev}/include";

      CARGO_BUILD_TARGET = rustTarget;
      "CARGO_TARGET_${upperTarget}_LINKER" = "${stdenv.cc.targetPrefix}cc";
      CARGO_BUILD_RUSTFLAGS = if isStatic then "-C target-feature=+crt-static" else "";
    };
in
{
  inherit bin devShell;
}
