# Build locked consumers; device/backend execution stays outside this derivation. The canonical tarball is a data input on both hosts.
{
  lib,
  stdenvNoCC,
  python311,
  zstd,
  sqlite,
  xmtp-pnpm,
  jsTools,
  cocoapods,
  expoCocoapods,
  rsync,
  perl,
  posixCommand,
  xmtp,
  rnShell,
  package,
  dependencies,
  template,
  nativeDependencies,
  consumer,
  platform,
  packageVersion,
  expectedSource,
}:
let
  root = ./../..;
  rn = root + /sdks/react-native;
  apple = import ../lib/react-native-env.nix { inherit lib; };
  podTool = if consumer == "expo" then expoCocoapods else cocoapods;
  source = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      (lib.fileset.fileFilter (file: !(file.hasExt "md") && file.name != "verification.json") (
        rn + "/test/consumers/${consumer}"
      ))
      (rn + /test/consumers/build.py)
      (rn + /test/consumers/prepare-template.py)
      (rn + /test/consumers/native-toolchain.json)
      (rn + /test/consumers/App.tsx)
      (rn + /test/consumers/native-hooks)
      (lib.fileset.maybeMissing (rn + /test/conformance/type-tests/tsconfig.json))
      (lib.fileset.maybeMissing (rn + /test/consumers/ios-downloads.json))
      (lib.fileset.maybeMissing (rn + /test/consumers/ios-spec-source.json))
      (lib.fileset.maybeMissing (rn + /test/consumers/offline-curl.py))
      (root + /sdks/node/test/standardSamples.ts)
      (lib.fileset.fileFilter (file: file.hasExt "ts") (rn + /test/conformance))
      (rn + /scripts/package.py)
      (root + /Cargo.toml)
      (root + /LICENSE)
      (root + /nix/sdk-platforms.json)
      (root + /crates/xmtp_sdk/dev/sdk-artifacts.py)
    ];
  };
in
stdenvNoCC.mkDerivation (
  finalAttrs:
  {
    pname = "react-native-consumer-${platform}-${consumer}";
    version = packageVersion;
    src = source;
    nativeBuildInputs = [
      python311
      zstd
      sqlite
      xmtp.nodejs_26
      xmtp-pnpm
      jsTools
    ]
    ++ (
      if platform == "ios" then
        [
          podTool
          rsync
          perl
          posixCommand
        ]
      else
        rnShell.nativeBuildInputs
    );
    ANDROID_HOME = lib.optionalString (platform == "android") rnShell.ANDROID_HOME;
    ANDROID_SDK_ROOT = lib.optionalString (platform == "android") rnShell.ANDROID_SDK_ROOT;
    ANDROID_NDK_HOME = lib.optionalString (platform == "android") rnShell.ANDROID_NDK_HOME;
    JAVA_HOME = lib.optionalString (platform == "android") rnShell.JAVA_HOME;
    XMTP_RN_ANDROID_TOOLCHAIN = lib.optionalString (
      platform == "android"
    ) rnShell.XMTP_RN_ANDROID_TOOLCHAIN;
    XMTP_NIX_ENV = "yes";
    XMTP_RN_TEMPLATE = template;
    XMTP_RN_TSC = "${jsTools}/bin/tsc";
    XMTP_RN_IOS_DOWNLOADS = lib.optionalString (platform == "ios") nativeDependencies;
    # Keep access to exactly the pinned host Xcode while retaining the network
    # sandbox. In particular, this is not an unsandboxed CocoaPods build.
    __impureHostDeps = lib.optionals (platform == "ios") [
      (builtins.dirOf apple.xcode.developerDir)
      # Apple ships this launcher outside Xcode. Expo and build.py both use it.
      "/usr/bin/xcrun"
      # Stock Expo shell phases use these exact Apple host executables.
      "/usr/bin/ditto"
      "/usr/bin/env"
      # Xcode hardcodes this executable for framework and app finalization.
      "/usr/bin/touch"
      # xcrun checks the existing license record; Xcode SDK discovery reads firmlinks.
      "/Library/Preferences/com.apple.dt.Xcode.plist"
      "/usr/share/firmlinks"
      # Xcode Simulator/device plug-ins load these exact host framework bundles.
      "/Library/Developer/PrivateFrameworks/CoreSimulator.framework"
      "/Library/Developer/PrivateFrameworks/DVTDownloads.framework"
      "/Library/Developer/PrivateFrameworks/ROCKit.framework"
      "/Library/Developer/PrivateFrameworks/CoreDevice.framework"
      "/Library/Developer/PrivateFrameworks/CoreDeviceUtilities.framework"
      "/Library/Apple/System/Library/PrivateFrameworks/MobileDevice.framework"
      "/Library/Apple/System/Library/PrivateFrameworks/Mercury.framework"
    ];
    mitmCache =
      if platform == "android" then
        rnShell.passthru.gradle.fetchDeps {
          pkg = finalAttrs.finalPackage;
          data = rn + "/test/consumers/${consumer}/android/gradle/dependencies.json";
        }
      else
        null;
    gradleFlags = lib.optionals (platform == "android") [
      "--dependency-verification=strict"
      "-Pkotlin.compiler.execution.strategy=in-process"
      "-Pandroid.aapt2FromMavenOverride=${rnShell.ANDROID_HOME}/build-tools/37.0.0/aapt2"
    ];
    passthru.nativeDependencies =
      if platform == "android" then finalAttrs.mitmCache else nativeDependencies;
    preConfigure = ''
      export HOME="$TMPDIR/home"
      mkdir -p "$HOME"
    '';
    buildPhase =
      let
        command = ''
          export HOME="$TMPDIR/home"
          export CP_HOME_DIR="$HOME/cocoapods"
          export CP_CACHE_DIR="$CP_HOME_DIR/cache"
          export GRADLE_USER_HOME="$HOME/gradle"
          export ANDROID_USER_HOME="$HOME/android"
          export PNPM_STORE_DIR="$TMPDIR/pnpm-store"
          mkdir -p "$HOME" "$PNPM_STORE_DIR" "$CP_CACHE_DIR" "$GRADLE_USER_HOME" "$ANDROID_USER_HOME"
          tar --zstd -xf ${dependencies}/pnpm-store.tar.zst -C "$PNPM_STORE_DIR"
          chmod -R u+w "$PNPM_STORE_DIR"
          if [ -f "$PNPM_STORE_DIR/v11/index.db.sql" ]; then
            sqlite3 "$PNPM_STORE_DIR/v11/index.db" < "$PNPM_STORE_DIR/v11/index.db.sql"
            rm "$PNPM_STORE_DIR/v11/index.db.sql"
          fi
          export pnpm_config_pm_on_fail=ignore
          export pnpm_config_trust_lockfile=true
          ${lib.optionalString (platform == "ios") (
            apple.checkXcode
            + ''
              export PATH="${
                lib.makeBinPath [
                  podTool
                  rsync
                  perl
                  posixCommand
                ]
              }:$PATH"
            ''
            + lib.optionalString (consumer == "expo") ''
              export __UNSAFE_EXPO_HOME_DIRECTORY="$TMPDIR/expo-cli-home"
            ''
          )}
          ${lib.optionalString (platform == "android") ''
            export XMTP_RN_GRADLE=${rnShell.passthru.gradle}/bin/gradle
            export XMTP_RN_GRADLE_REPLAY=1
            flags=()
            concatTo flags gradleFlags gradleFlagsArray
            # R8 runs in the Gradle build JVM, and the templates' 2 GiB heap
            # runs out. Use the 4 GiB heap of the repository's other Android builds.
            flags+=("-Dorg.gradle.jvmargs=-Xmx4096m -XX:MaxMetaspaceSize=1024m")
            export XMTP_RN_GRADLE_FLAGS_JSON=$(python3.11 -c 'import json,sys; print(json.dumps(sys.argv[1:]))' "''${flags[@]}")
          ''}
          package=$(find ${package} -maxdepth 1 -type f -name '*.tgz')
          test -n "$package"
          python3.11 sdks/react-native/test/consumers/build.py \
            --platform ${platform} --consumer ${consumer} --package "$package" \
            --expected-version ${lib.escapeShellArg packageVersion} \
            --expected-source ${lib.escapeShellArg expectedSource} \
            --work-dir "$TMPDIR/app" --output "$TMPDIR/bundle" --offline
        '';
      in
      if platform == "ios" then
        lib.replaceStrings
          [
            "python3.11 sdks/react-native/test/consumers/build.py \\"
            ''--work-dir "$TMPDIR/app" --output "$TMPDIR/bundle" --offline''
          ]
          [
            (lib.concatStringsSep "\n" [
              "set -o pipefail"
              "if python3.11 sdks/react-native/test/consumers/build.py \\"
            ])
            (lib.concatStringsSep "\n" [
              ''--work-dir "$TMPDIR/app" --output "$TMPDIR/bundle" --offline 2>&1 | tee "$TMPDIR/consumer-build.log"; then''
              "  :"
              "else"
              ''statuses=( "''${PIPESTATUS[@]}" )''
              "status=\${statuses[0]}"
              ''if [ "$status" -eq 0 ]; then status=''${statuses[1]}; fi''
              ''python3.11 ${./react-native-consumer-log.py} "$TMPDIR/consumer-build.log" || true''
              ''exit "$status"''
              "fi"
            ])
          ]
          command
      else
        command;
    installPhase = ''
      mv "$TMPDIR/bundle" "$out"
    '';
    dontFixup = true;
  }
  // lib.optionalAttrs (platform == "ios") {
    # Nix requires command-local sandbox=relaxed for this additive read-only rule.
    # These app roots still use the Darwin sandbox and cannot opt out of it.
    __noChroot = false;
    sandboxProfile = ''
      (allow mach-lookup (global-name "com.apple.lsd.mapdb"))
    '';
  }
)
