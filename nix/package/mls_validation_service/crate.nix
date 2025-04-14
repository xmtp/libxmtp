{ openssl
, sqlite
, sqlcipher
, perl
, lib
, pkg-config
, stdenv
, craneLib
, darwin
, filesets
, mkShell
, rustTarget ? null
}:
let
  inherit (stdenv) hostPlatform;
  upperTarget = lib.strings.toUpper (builtins.replaceStrings [ "-" ] [ "_" ] rustTarget);
  pname = (builtins.fromTOML (builtins.readFile ./../../../mls_validation_service/Cargo.toml)).package.name;
  version = (builtins.fromTOML (builtins.readFile ./../../../mls_validation_service/Cargo.toml)).package.version;


  filesetForCrate = crate: lib.fileset.toSource {
    root = ./../../..;
    fileset = filesets.filesetForCrate crate;
  };

  filesetForWorkspace = lib.fileset.toSource {
    root = ./../../..;
    fileset = filesets.workspace;
  };

  linkerArgs = if rustTarget == null then { } else {
    HOST_CC = "${stdenv.cc.nativePrefix}cc";
    TARGET_CC = "${stdenv.cc.targetPrefix}cc";
    # https://docs.rs/openssl/latest/openssl/#manual
    "${upperTarget}_OPENSSL_DIR" = "${openssl.out}";
    "${upperTarget}_OPENSSL_LIB_DIR" = "${openssl.out}/lib";
    "${upperTarget}_OPENSSL_INCLUDE_DIR" = "${openssl.dev}/include";
    # Important because otherwise nix won't be able to link in the correct
    # openssl library from the cross pkg set
    OPENSSL_NO_VENDOR = 1;
    CARGO_BUILD_TARGET = rustTarget;
    "CARGO_TARGET_${upperTarget}_LINKER" = "${stdenv.cc.targetPrefix}cc";
  };

  commonArgs = {
    src = filesetForWorkspace;
    strictDeps = true;

    # Used to build on the current/build machine
    nativeBuildInputs = [
      pkg-config
      stdenv.cc
      perl
    ];

    # Libraries that will run on the host machine
    # that to be linked
    buildInputs = [
      # Add additional build inputs here
      openssl
      sqlite
      sqlcipher
    ] ++ lib.optionals hostPlatform.isDarwin
      (with darwin.apple_sdk;
      [
        frameworks.Security
        frameworks.SystemConfiguration
      ]);

    doCheck = false;
    cargoExtraArgs = "--workspace --exclude xmtpv3 --exclude bindings_node --exclude bindings_wasm --exclude xmtp_cli";
    RUSTFLAGS = [ "--cfg" "tracing_unstable" ];
    OPENSSL_DIR = "${openssl.out}";
    OPENSSL_INCLUDE_DIR = "${openssl.dev}/include";
  } // linkerArgs;

  cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
    pname = "mls-validation-service-deps";
  });

  bin = craneLib.buildPackage ({
    inherit cargoArtifacts pname version;
    cargoExtraArgs = "--package mls_validation_service";
    src = filesetForCrate ./../../../mls_validation_service;
    doCheck = false;

    RUST_BACKTRACE = 1;
  } // commonArgs);

  devShell = mkShell
    {
      inputsFrom = [ bin ];
    };
in
{
  inherit bin devShell;
}
