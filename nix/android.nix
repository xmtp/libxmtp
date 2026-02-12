{ mkShell
, darwin
, stdenv
, kotlin
, ktlint
, jdk17
, cargo-ndk
, openssl
, lib
, gnused
, xmtp
, writeShellScriptBin
, zlib
}:
let
  inherit (xmtp) androidEnv mobile;
  # Use dev composition (includes emulator)
  androidComposition = androidEnv.composeDevPackages;
  androidPaths = androidEnv.mkAndroidPaths androidComposition;

  # Rust toolchain with Android cross-compilation targets
  rust-android-toolchain = xmtp.mkToolchain androidEnv.androidTargets [ "clippy-preview" "rustfmt-preview" ];

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
  androidSdk = "${androidComposition.androidsdk}/libexec/android-sdk";
  inherit (androidEnv.emulatorConfig) platformVersion abiVersion systemImageType;

  androidEmulator = writeShellScriptBin "run-test-emulator" ''
    set -e

    ADB="${androidSdk}/platform-tools/adb"
    EMULATOR_BIN="${androidSdk}/emulator/emulator"
    AVDMANAGER="${androidComposition.androidsdk}/bin/avdmanager"

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
      -k "system-images;android-${platformVersion};${systemImageType};${abiVersion}" \
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
in
mkShell {
  meta.description = "Android Development environment for Android SDK and Emulator";

  XMTP_DEV_SHELL = "android";
  OPENSSL_DIR = "${openssl.dev}";
  ANDROID_HOME = androidPaths.home;
  ANDROID_SDK_ROOT = androidPaths.home;
  ANDROID_NDK_HOME = androidPaths.ndkHome;
  ANDROID_NDK_ROOT = androidPaths.ndkHome;
  NDK_HOME = androidPaths.ndkHome;
  EMULATOR = "${androidEmulator}";
  LD_LIBRARY_PATH = lib.makeLibraryPath [ openssl zlib ];

  inherit (mobile.commonArgs) nativeBuildInputs;

  buildInputs = mobile.commonArgs.buildInputs ++ [
    rust-android-toolchain
    kotlin
    ktlint
    androidComposition.androidsdk
    jdk17
    cargo-ndk
    androidEmulator
    gnused
  ] ++ lib.optionals stdenv.isDarwin [
    darwin.cctools
  ];
}
