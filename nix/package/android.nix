# Android cross-compilation package derivation.
# Builds shared libraries (.so) for 4 Android targets + Kotlin bindings, cacheable in Cachix.
# Uses shared config from nix/lib/mobile-common.nix.
#
# Uses cargo-ndk for cross-compilation which handles OpenSSL and other native
# dependencies correctly for all Android targets including 32-bit ARM.
#
# Architecture:
# - Per-target derivations are built in parallel via lib.genAttrs
# - Each target has its own deps derivation + build derivation
# - Kotlin bindings are built separately on the host
# - Aggregate derivation symlinks all outputs together
#
# This produces:
#   - targets.{target-name}: Individual .so files per target
#   - kotlinBindings: Kotlin bindings + version file
#   - aggregate: Combined output with jniLibs/{ABI}/*.so and kotlin/*
{ lib
, zstd
, openssl
, sqlite
, pkg-config
, perl
, gnused
, craneLib
, xmtp
, stdenv
, androidenv
, cargo-ndk
, zlib
, ...
}:
let
  # Shared Android environment configuration
  androidEnv = import ./../lib/android-env.nix { inherit lib androidenv; };

  # Use build composition (minimal - no emulator needed for CI builds)
  androidComposition = androidEnv.composeBuildPackages;
  androidPaths = androidEnv.mkAndroidPaths androidComposition;

  # Shared mobile build configuration (commonArgs, filesets, version)
  mobile = import ./../lib/mobile-common.nix {
    inherit lib craneLib xmtp zstd openssl sqlite pkg-config perl zlib;
  };

  # Rust toolchain with Android cross-compilation targets
  rust-toolchain = xmtp.mkToolchain androidEnv.androidTargets [];
  rust = craneLib.overrideToolchain (p: rust-toolchain);

  # Extract version once for use throughout the file
  version = mobile.mkVersion rust;

  # Inherit shared config
  inherit (mobile) depsFileset bindingsFileset;

  # Map Rust target triples to Android ABI names
  targetToAbi = {
    "aarch64-linux-android" = "arm64-v8a";
    "armv7-linux-androideabi" = "armeabi-v7a";
    "x86_64-linux-android" = "x86_64";
    "i686-linux-android" = "x86";
  };

  # Android-specific commonArgs extends the shared config with NDK environment
  commonArgs = mobile.commonArgs // {
    nativeBuildInputs = mobile.commonArgs.nativeBuildInputs ++ [ cargo-ndk ];

    # Android NDK environment
    ANDROID_HOME = androidPaths.home;
    ANDROID_NDK_HOME = androidPaths.ndkHome;
    ANDROID_NDK_ROOT = androidPaths.ndkHome;
    OPENSSL_DIR = "${openssl.dev}";
  };

  # Build dependencies for a specific Android target
  buildTargetDeps = target:
    rust.buildDepsOnly (commonArgs // {
      pname = "xmtpv3-android-deps-${target}";
      cargoExtraArgs = "-p xmtpv3";

      # Build deps for this specific target using cargo-ndk
      buildPhaseCargoCommand = ''
        cargo ndk --platform 23 -t ${target} \
          --manifest-path ./bindings/mobile/Cargo.toml \
          -- build --release
      '';
    });

  # Build a single Android target
  buildTarget = target:
    let
      abi = targetToAbi.${target};
      cargoArtifacts = buildTargetDeps target;
    in
    rust.buildPackage (commonArgs // {
      inherit cargoArtifacts version;
      pname = "xmtpv3-${target}";
      src = bindingsFileset;
      cargoExtraArgs = "-p xmtpv3";

      buildPhaseCargoCommand = ''
        cargo ndk --platform 23 -t ${target} \
          --manifest-path ./bindings/mobile/Cargo.toml \
          -o $TMPDIR/jniLibs -- build --release
      '';

      doNotPostBuildInstallCargoBinaries = true;

      installPhaseCommand = ''
        mkdir -p $out/${abi}
        cp $TMPDIR/jniLibs/${abi}/libxmtpv3.so $out/${abi}/libuniffi_xmtpv3.so
      '';
    });

  # Generate per-target derivations (built in parallel by Nix)
  targets = lib.genAttrs androidEnv.androidTargets buildTarget;

  # Build dependencies for the native host (needed for uniffi-bindgen)
  hostCargoArtifacts = rust.buildDepsOnly (commonArgs // {
    pname = "xmtpv3-android-host-deps";
    cargoExtraArgs = "-p xmtpv3";
  });

  # Kotlin bindings (built on host, generates bindings from host library)
  kotlinBindings = rust.buildPackage (commonArgs // {
    pname = "xmtpv3-kotlin-bindings";
    inherit version;
    src = bindingsFileset;
    cargoArtifacts = hostCargoArtifacts;
    cargoExtraArgs = "-p xmtpv3";

    nativeBuildInputs = commonArgs.nativeBuildInputs ++ [ gnused ];

    buildPhaseCargoCommand = ''
      cargo build --release -p xmtpv3
    '';

    doNotPostBuildInstallCargoBinaries = true;

    installPhaseCommand = ''
      mkdir -p $out/kotlin

      # Generate Kotlin bindings using uniffi-bindgen
      cargo run -p xmtpv3 --bin ffi-uniffi-bindgen --release --features uniffi/cli generate \
        --library target/release/libxmtpv3.${if stdenv.isDarwin then "dylib" else "so"} \
        --out-dir $TMPDIR/kotlin-out \
        --language kotlin

      # Apply required sed replacements:
      # 1) Replace `return "xmtpv3"` with `return "uniffi_xmtpv3"` (library name fix)
      # 2) Replace `value.forEach { (k, v) ->` with `value.iterator().forEach { (k, v) ->`
      # Note: uniffi outputs to uniffi/<crate_name>/<crate_name>.kt
      sed -i \
        -e 's/return "xmtpv3"/return "uniffi_xmtpv3"/' \
        -e 's/value\.forEach { (k, v) ->/value.iterator().forEach { (k, v) ->/g' \
        "$TMPDIR/kotlin-out/uniffi/xmtpv3/xmtpv3.kt"

      cp $TMPDIR/kotlin-out/uniffi/xmtpv3/xmtpv3.kt $out/kotlin/

      # Generate version file
      echo "Version: ${version}" > $out/kotlin/libxmtp-version.txt
      echo "Date: $(date -u +%Y-%m-%d)" >> $out/kotlin/libxmtp-version.txt
    '';
  });

  # Aggregate derivation that symlinks all outputs together
  # This is a pure derivation - no compilation, just links
  aggregate = stdenv.mkDerivation {
    pname = "xmtpv3-android-libs";
    inherit version;

    # No source needed - we just symlink outputs
    dontUnpack = true;

    installPhase = ''
      mkdir -p $out/jniLibs $out/kotlin

      # Symlink JNI libraries from each target
      ${lib.concatMapStringsSep "\n" (target:
        let abi = targetToAbi.${target}; in ''
          mkdir -p $out/jniLibs/${abi}
          ln -s ${targets.${target}}/${abi}/libuniffi_xmtpv3.so $out/jniLibs/${abi}/libuniffi_xmtpv3.so
        '') androidEnv.androidTargets}

      # Symlink Kotlin bindings
      ln -s ${kotlinBindings}/kotlin/xmtpv3.kt $out/kotlin/xmtpv3.kt
      ln -s ${kotlinBindings}/kotlin/libxmtp-version.txt $out/kotlin/libxmtp-version.txt
    '';
  };

in
{
  inherit targets kotlinBindings aggregate;
}
