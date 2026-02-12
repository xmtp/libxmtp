# Shared Android cross-compilation environment configuration.
# Used by both nix/shells/android.nix (dev shell) and nix/package/android.nix (build derivation).
{ lib, androidenv, stdenv, writeShellScriptBin }:
let
  # Android targets for Rust cross-compilation
  androidTargets = [
    "aarch64-linux-android"
    "armv7-linux-androideabi"
    "x86_64-linux-android"
    "i686-linux-android"
  ];

  # Host architecture -> matching Android target (for fast single-target builds)
  hostArch = stdenv.hostPlatform.parsed.cpu.name;
  hostAndroidTarget = {
    "aarch64" = "aarch64-linux-android";
    "x86_64" = "x86_64-linux-android";
  }.${hostArch} or (throw "Unsupported host architecture for Android: ${hostArch}");

  # SDK configuration - keep in sync with sdks/android/library/build.gradle
  # Library: compileSdk 35, Example: compileSdk 34
  # Gradle auto-selects buildTools matching compileSdk when not specified.
  sdkConfig = {
    platforms = [ "34" "35" ];
    platformTools = "35.0.2";
    buildTools = [ "34.0.0" "35.0.0" ];
  };

  # Compose Android packages for builds (minimal - no emulator)
  composeBuildPackages = androidenv.composeAndroidPackages {
    platformVersions = sdkConfig.platforms;
    platformToolsVersion = sdkConfig.platformTools;
    buildToolsVersions = sdkConfig.buildTools;
    includeNDK = true;
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

  # Compose Android packages for dev shell (includes emulator)
  composeDevPackages = androidenv.composeAndroidPackages {
    platformVersions = sdkConfig.platforms;
    platformToolsVersion = sdkConfig.platformTools;
    buildToolsVersions = sdkConfig.buildTools;
    includeNDK = true;
    inherit (emulatorConfig) emulatorVersion;
    includeEmulator = true;
    includeSystemImages = true;
    systemImageTypes = [ emulatorConfig.systemImageType ];
    abiVersions = [ emulatorConfig.abiVersion ];
  };

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
  #
  # Why: In CI, ./dev/docker/up starts Docker services on ports that overlap
  # with the Android emulator's default port scan range (5554-5584):
  #   - 5555: node (gRPC)    - 5557: node-web
  #   - 5556: node            - 5558: history-server
  #
  # The nixpkgs emulateApp port scanner only checks `adb devices` output,
  # not whether ports are actually bindable. ADB's auto-discovery misidentifies
  # Docker services on odd ports (5555, 5557) as emulators, causing the scanner
  # to skip ports 5554 and 5556 and land on 5558 — which is occupied by the
  # history-server. The emulator then fails to bind its console port, can't
  # register with ADB, and `adb wait-for-device` hangs until the CI timeout.
  #
  # Fix: Start scanning at port 5560, above all Docker service ports.
  emulator = writeShellScriptBin "run-test-emulator" ''
    set -e

    ADB="${androidSdk}/platform-tools/adb"
    EMULATOR_BIN="${androidSdk}/emulator/emulator"
    AVDMANAGER="${composeDevPackages.androidsdk}/bin/avdmanager"

    export ANDROID_SDK_ROOT="${androidSdk}"
    export ANDROID_USER_HOME=$(mktemp -d "''${TMPDIR:-/tmp}/nix-android-user-home-XXXX")
    export ANDROID_AVD_HOME="$ANDROID_USER_HOME/avd"
    mkdir -p "$ANDROID_AVD_HOME"

    DEVICE_NAME="libxmtp-test"

    if [ -z "$NIX_ANDROID_EMULATOR_FLAGS" ]; then
      NIX_ANDROID_EMULATOR_FLAGS="-no-snapshot-save -gpu swiftshader_indirect -memory 4096 -partition-size 8192"
    fi

    # Scan ports 5560-5584 to avoid conflicts with Docker services (5555-5558)
    echo "Looking for a free TCP port in range 5560-5584" >&2
    port=""
    for i in $(seq 5560 2 5584); do
      if ! "$ADB" devices 2>/dev/null | grep -q "emulator-$i"; then
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
    yes "" | "$AVDMANAGER" create avd \
      --force -n "$DEVICE_NAME" \
      -k "system-images;android-${emulatorConfig.platformVersion};${emulatorConfig.systemImageType};${emulatorConfig.abiVersion}" \
      -p "$ANDROID_AVD_HOME/$DEVICE_NAME.avd"

    # Hardware config
    {
      echo "hw.gpu.enabled = yes"
      echo "hw.gpu.mode = swiftshader_indirect"
      echo "hw.ramSize = 4096"
      echo "disk.dataPartition.size = 8192M"
    } >> "$ANDROID_AVD_HOME/$DEVICE_NAME.avd/config.ini"

    # Launch emulator in background
    "$EMULATOR_BIN" -avd "$DEVICE_NAME" -no-boot-anim -port "$port" $NIX_ANDROID_EMULATOR_FLAGS &

    # Wait for device to appear
    "$ADB" -s "emulator-$port" wait-for-device

    # Wait for boot to complete
    while [ -z "$("$ADB" -s "emulator-$port" shell getprop dev.bootcomplete 2>/dev/null | grep 1)" ]; do
      sleep 5
    done

    echo "Emulator ready (emulator-$port)" >&2
  '';

in {
  inherit androidTargets hostAndroidTarget sdkConfig emulatorConfig
    composeBuildPackages composeDevPackages mkAndroidPaths
    devComposition devPaths emulator;
}
