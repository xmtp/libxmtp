{ mkShell
, darwin
, androidenv
, stdenv
, pkg-config
, kotlin
, ktlint
, jdk17
, cargo-ndk
, sqlite
, sqlcipher
, openssl
, lib
, gnused
, perl
, craneLib
, zstd
, zlib
, libz
, git
, xmtp
}:
let
  inherit (androidComposition) androidsdk;

  android = {
    platforms = [ "33" "34" ];
    platformTools = "34.0.4";
    buildTools = [ "30.0.3" ];
  };

  androidTargets = [
    "aarch64-linux-android"
    "armv7-linux-androideabi"
    "x86_64-linux-android"
    "i686-linux-android"
  ];

  # Helper function to generate Android target environment variables
  mkAndroidTargetVars = target:
    let
      # Convert target name to env var format (replace - with _)
      envTarget = lib.replaceStrings [ "-" ] [ "_" ] target;
      # Map target to NDK prefix
      ndkPrefix = {
        "aarch64-linux-android" = "aarch64-linux-android23";
        "armv7-linux-androideabi" = "armv7a-linux-androideabi23";
        "x86_64-linux-android" = "x86_64-linux-android23";
        "i686-linux-android" = "i686-linux-android23";
      }.${target};
    in
    ''
      export CC_${envTarget}="${androidHome}/ndk-bundle/toolchains/llvm/prebuilt/linux-x86_64/bin/${ndkPrefix}-clang"
      export CXX_${envTarget}="${androidHome}/ndk-bundle/toolchains/llvm/prebuilt/linux-x86_64/bin/${ndkPrefix}-clang++"
      export AR_${envTarget}="${androidHome}/ndk-bundle/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
    '';

  filesets = xmtp.filesets { inherit lib craneLib; };
  workspaceFileset = lib.fileset.toSource {
    root = ./..;
    fileset = filesets.workspace;
  };

  # Pinned Rust Version
  rust-toolchain = xmtp.mkToolchain androidTargets [ "clippy-preview" "rustfmt-preview" "rust-src" ];
  rust = craneLib.overrideToolchain (p: rust-toolchain);
  sdkArgs = {
    platformVersions = android.platforms;
    platformToolsVersion = android.platformTools;
    buildToolsVersions = android.buildTools;
    emulatorVersion = "34.1.9";
    systemImageTypes = [ "google_apis_playstore" "default" ];
    abiVersions = [ "x86_64" ];
    includeNDK = true;
    includeEmulator = true;
    includeSystemImages = true;
  };

  # https://github.com/NixOS/nixpkgs/blob/master/doc/languages-frameworks/android.section.md
  androidHome = "${androidComposition.androidsdk}/libexec/android-sdk";
  androidComposition = androidenv.composeAndroidPackages sdkArgs;
  androidEmulator = androidenv.emulateApp {
    name = "libxmtp-emulator-34";
    platformVersion = "34";
    abiVersion = "x86_64"; # armeabi-v7a, mips, x86_64
    systemImageType = "default";
  };

  commonInputs = {
    OPENSSL_DIR = "${openssl.dev}";
    ANDROID_HOME = androidHome;
    NDK_HOME = "${androidComposition.androidsdk}/libexec/android-sdk/ndk/${builtins.head (lib.lists.reverseList (builtins.split "-" "${androidComposition.ndk-bundle}"))}";
    ANDROID_SDK_ROOT = androidHome; # ANDROID_SDK_ROOT is deprecated, but some tools may still use it;
    ANDROID_NDK_ROOT = "${androidHome}/ndk-bundle";

    # Packages available to derivation while building the environment
    nativeBuildInputs = [ pkg-config perl cargo-ndk ];
  };

  buildInputs = [
    androidsdk
    rust-toolchain
    kotlin
    jdk17
    cargo-ndk
    gnused
    perl
    ktlint
    git

    sqlite
    sqlcipher
    openssl
    zstd
    zlib
    libz
  ];

  devShell = mkShell
    {
      buildInputs = [
        ktlint
        androidEmulator
      ] ++ buildInputs ++ lib.optionals stdenv.isDarwin [
        # optional packages if on darwin, in order to check if build passes locally
        darwin.cctools
      ];
    } // commonInputs;

  # dylib build
  commonArgs = {
    src = rust.cleanCargoSource ./..;
    cargoLock = ./../Cargo.lock;
    CargoToml = ./../bindings_ffi/Cargo.toml;
    strictDeps = true;
    inherit buildInputs;

    doCheck = false;
    cargoExtraArgs = "--workspace --exclude bindings_wasm --exclude bindings_node --exclude xmtp_cli --exclude xdbg --exclude mls_validation_service --exclude xmtp_api_http";
    RUSTFLAGS = [ "--cfg" "tracing_unstable" ];
  } // commonInputs;

  # enables caching all build time crates
  cargoArtifacts = rust.buildDepsOnly (commonArgs // {
    doCheck = false;
  });

  # building it outside of jniLibs lets nix cache the binary
  uniffiBindgen = rust.buildPackage (commonArgs // {
    inherit cargoArtifacts;
    src = workspaceFileset;
    pname = "ffi-uniffi-bindgen";
    inherit (rust.crateNameFromCargoToml {
      cargoToml = ./../Cargo.toml;
    }) version;
    cargoExtraArgs = "--bin ffi-uniffi-bindgen";
  });

  jniLibs = rust.buildPackage (commonArgs // {
    inherit cargoArtifacts;
    src = workspaceFileset;
    pname = "xmtpv3";
    inherit (rust.crateNameFromCargoToml {
      cargoToml = ./../Cargo.toml;
    }) version;

    buildPhaseCargoCommand = ''
      mkdir -p $out/jniLibs
      cargoBuildLog=$(mktemp cargoBuildLogXXXX.json)
      projectName="xmtpv3"
      libName="uniffi_xmtpv3"
      libFile=$([ "$(uname)" == "Darwin" ] && echo "lib''${libName}.dylib" || echo "lib''${libName}.so")

      cd bindings_ffi/
      make libxmtp-version
      cd ../

      cargo build --release -p xmtpv3 --locked --message-format json-render-diagnostics > "$cargoBuildLog"

      ${uniffiBindgen}/bin/ffi-uniffi-bindgen generate \
        --library target/release/$libFile \
        --out-dir $out \
        --language kotlin

      export LIBZ_SYS_STATIC=0
      export PKG_CONFIG_ALLOW_CROSS=1

      # Set up toolchain environment variables for all Android targets
      ${lib.concatStringsSep "\n" (map mkAndroidTargetVars androidTargets)}

      cargo ndk --platform 23 -o $out/jniLibs/ --manifest-path ./bindings_ffi/Cargo.toml \
        -t aarch64-linux-android \
        -t armv7-linux-androideabi \
        -t x86_64-linux-android \
        -t i686-linux-android \
        -- build --release \
        --message-format json-render-diagnostics >> "$cargoBuildLog"
      cp bindings_ffi/libxmtp-version.txt $out/
    '';
  });
in
{
  inherit devShell jniLibs androidEmulator;
}


