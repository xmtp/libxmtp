# Common args for building xmtp crates
{ openssl
, sqlite
, sqlcipher
, perl
, lib
, pkg-config
, stdenv
, craneLib
, darwin
, xmtp
, rustTarget ? null
, cargoExtraArgs ? ""
, ...
}:
let
  inherit (stdenv) hostPlatform;
  upperTarget = lib.strings.toUpper (builtins.replaceStrings [ "-" ] [ "_" ] rustTarget);
  crateFilesets = xmtp.filesets { inherit lib craneLib; };
  filesetForWorkspace = lib.fileset.toSource {
    root = ./../..;
    fileset = crateFilesets.workspace;
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
    CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
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
    RUSTFLAGS = [ "--cfg" "tracing_unstable" ];
    OPENSSL_DIR = "${openssl.out}";
    OPENSSL_INCLUDE_DIR = "${openssl.dev}/include";
  } // linkerArgs;
  cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
    cargoExtraArgs = "--workspace --lib --exclude bindings_node --exclude xmtpv3 --exclude bindings_wasm ${cargoExtraArgs}";
  });
in
{
  inherit commonArgs cargoArtifacts craneLib;
}

