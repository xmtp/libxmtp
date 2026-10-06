# Shared Android cross-compilation environment configuration.
# Used by both nix/shells/android.nix (dev shell) and nix/package/android.nix (build derivation).
{
  lib,
  androidenv,
  stdenv,
  writeShellScriptBin,
  coreutils,
  python3,
}:
let
  androidTargets = [
    "aarch64-linux-android"
    "armv7-linux-androideabi"
    "x86_64-linux-android"
    "i686-linux-android"
  ];
  # Host architecture -> matching Android target (for fast single-target builds)
  hostArch = stdenv.hostPlatform.parsed.cpu.name;
  # The Android emulator is only available for x86_64-linux and *-darwin.
  # aarch64-linux has no emulator binary in the Android SDK.
  hasEmulator = !stdenv.isLinux || hostArch == "x86_64";
  hasApi23Emulator = stdenv.isLinux && hostArch == "x86_64";

  # SDK configuration - keep in sync with sdks/android/library/build.gradle
  # Library: compileSdk 35, Example: compileSdk 34
  # Gradle auto-selects buildTools matching compileSdk when not specified.
  sdkConfig = {
    platforms = [
      "34"
      "35"
    ]
    ++ lib.optionals hasApi23Emulator [ "23" ];
    platformTools = "35.0.2";
    buildTools = [
      "34.0.0"
      "35.0.0"
    ];
  };

  # Emulator configuration — used by both composeDevPackages and run-test-emulator
  # Version >= 35.3.11 required: earlier versions lack arch metadata in nixpkgs'
  # repo.json, so aarch64-darwin gets an x86_64 binary that can't run arm64 guests.
  # With 35.3.11+, nixpkgs selects the correct native binary per architecture.
  emulatorConfig = {
    platformVersion = "34";
    systemImageType = "default";
    abiVersion = if hostArch == "aarch64" then "arm64-v8a" else "x86_64";
    emulatorVersion = "35.3.11";
  };

  # Compose Android packages for dev shell (includes emulator where available)
  composeDevPackages = androidenv.composeAndroidPackages (
    {
      platformVersions = sdkConfig.platforms;
      platformToolsVersion = sdkConfig.platformTools;
      buildToolsVersions = sdkConfig.buildTools;
      includeNDK = true;
    }
    // lib.optionalAttrs hasEmulator {
      inherit (emulatorConfig) emulatorVersion;
      includeEmulator = true;
      includeSystemImages = true;
      systemImageTypes = [ emulatorConfig.systemImageType ];
      abiVersions = [ emulatorConfig.abiVersion ];
    }
  );

  # Helper to extract paths from an android composition
  mkAndroidPaths = composition: rec {
    home = "${composition.androidsdk}/libexec/android-sdk";
    # NDK version extraction from the ndk-bundle attribute name
    ndkVersion = builtins.head (lib.lists.reverseList (builtins.split "-" "${composition.ndk-bundle}"));
    ndkHome = "${home}/ndk/${ndkVersion}";
  };

  # --- Dev shell helpers (shared between android.nix and local.nix) ---
  devComposition = composeDevPackages;
  devPaths = mkAndroidPaths composeDevPackages;
  androidSdk = "${composeDevPackages.androidsdk}/libexec/android-sdk";

  # Custom emulator launch script replacing nixpkgs' androidenv.emulateApp.
  # Only defined on platforms where the Android emulator is available.
  #
  # Keep the established emulator port range for local and CI runs.
  emulator = writeShellScriptBin "run-test-emulator" ''
    set -e
    export PATH="${lib.makeBinPath [ coreutils ]}:$PATH"

    ADB="${androidSdk}/platform-tools/adb"
    EMULATOR_BIN="${androidSdk}/emulator/emulator"
    AVDMANAGER="${composeDevPackages.androidsdk}/bin/avdmanager"
    ANDROID_DEFAULT_EMULATOR_API="${emulatorConfig.platformVersion}"
    ANDROID_API23_SUPPORTED="${if hasApi23Emulator then "1" else "0"}"
    source ${./android-emulator-platform.sh}

    export ANDROID_SDK_ROOT="${androidSdk}"
    export ANDROID_USER_HOME=$(mktemp -d "''${TMPDIR:-/tmp}/nix-android-user-home-XXXX")
    export ANDROID_AVD_HOME="$ANDROID_USER_HOME/avd"
    mkdir -p "$ANDROID_AVD_HOME"

    DEVICE_NAME="libxmtp-test"

    if [ -z "$NIX_ANDROID_EMULATOR_FLAGS" ]; then
      NIX_ANDROID_EMULATOR_FLAGS="-no-snapshot-save -gpu swiftshader_indirect -memory 4096 -partition-size 8192"
    fi

    # Scan ports 5560-5584 to avoid conflicts with Docker services (5050, 6010, 8474, 8545)
    echo "Looking for a free TCP port in range 5560-5584" >&2
    port=""
    for i in $(seq 5560 2 5584); do
      devices="$(timeout --kill-after=2 15 "$ADB" devices)"
      if ! echo "$devices" | grep -q "emulator-$i"; then
        port=$i
        break
      fi
    done

    if [ -z "$port" ]; then
      echo "No free emulator port found!" >&2
      exit 1
    fi
    echo "Using emulator port: $port" >&2

    export ANDROID_SERIAL="emulator-$port"

    # Create AVD
    printf 'no\n' | timeout --kill-after=2 60 "$AVDMANAGER" create avd \
      --force -n "$DEVICE_NAME" \
      -k "system-images;android-$ANDROID_EMULATOR_API;${emulatorConfig.systemImageType};${emulatorConfig.abiVersion}" \
      -p "$ANDROID_AVD_HOME/$DEVICE_NAME.avd"

    # Hardware config
    {
      echo "hw.gpu.enabled = yes"
      echo "hw.gpu.mode = swiftshader_indirect"
      echo "hw.ramSize = 4096"
      echo "disk.dataPartition.size = 8192M"
    } >> "$ANDROID_AVD_HOME/$DEVICE_NAME.avd/config.ini"

    # The supervisor checks process death even while ADB or clock sync is blocked.
    exec ${python3}/bin/python3 ${./android-emulator-start.py} \
      "$ADB" "$EMULATOR_BIN" "$DEVICE_NAME" "$ANDROID_SERIAL" \
      "$ANDROID_EMULATOR_API" ${./android-sync-clock.sh} $NIX_ANDROID_EMULATOR_FLAGS
  '';

in
{
  inherit
    androidTargets
    sdkConfig
    emulatorConfig
    composeDevPackages
    devComposition
    devPaths
    hasEmulator
    ;
}
// lib.optionalAttrs hasEmulator {
  inherit emulator;
}
