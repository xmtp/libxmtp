# RN/Expo native tools are separate from the sibling Android/iOS toolchains.
{
  pkgs,
  lib,
  mkShell,
  nodejs_26,
  xmtp-pnpm,
  python311,
  jdk21,
  jdk17,
  cocoapods,
  rsync,
  perl,
  androidenv,
  gradle-packages,
  path,
  mitm-cache,
  openssl,
  local ? false,
}:
let
  apple = import ../lib/react-native-env.nix { inherit lib; };
  posixCommand = pkgs.callPackage ../package/react-native-command.nix { };
  tools = lib.importJSON ../../sdks/react-native/test/consumers/native-toolchain.json;
  android = androidenv.composeAndroidPackages {
    platformVersions = [ (toString tools.android.compileSdk) ];
    buildToolsVersions = [ tools.android.buildTools ];
    cmakeVersions = [ tools.android.cmake ];
    includeNDK = true;
    ndkVersions = [ tools.android.ndk ];
    includeEmulator = false;
    includeSystemImages = false;
  };
  gradleDistribution =
    (
      (gradle-packages.mkGradle {
        version = tools.gradle.version;
        hash = "sha256:${tools.gradle.sha256}";
        defaultJava = jdk21;
      }).override
      {
        # Gradle installation properties also apply to the included RN plugin.
        javaToolchains = [
          jdk17.home
          jdk21.home
        ];
      }
    ).overrideAttrs
      (old: {
        installPhase = old.installPhase + ''
          cat >> "$out/libexec/gradle/gradle.properties" <<EOF
          org.gradle.java.installations.auto-detect=false
          org.gradle.java.installations.auto-download=false
          EOF
        '';
      });
  gradle = pkgs.callPackage gradle-packages.wrapGradle {
    gradle-unwrapped = gradleDistribution;
  };
  javaIdentity = jdk: {
    javaVersion = jdk.version;
    derivationHash = builtins.hashString "sha256" (builtins.unsafeDiscardStringContext jdk.drvPath);
  };
  androidToolchain = builtins.toJSON {
    schema = 1;
    gradle = {
      version = tools.gradle.version;
      derivationHash = builtins.hashString "sha256" (
        builtins.unsafeDiscardStringContext gradleDistribution.drvPath
      );
    };
    runtime = javaIdentity jdk21;
    compiler = javaIdentity jdk17;
    policy = {
      autoDetect = false;
      autoDownload = false;
    };
  };
  androidHome = "${android.androidsdk}/libexec/android-sdk";
in
mkShell {
  name = "xmtp-react-native${lib.optionalString local "-local"}";
  shellHook = lib.optionalString pkgs.stdenv.isDarwin (
    (if local then apple.localXcode else apple.checkXcode)
    + ''
      export PATH="${
        lib.makeBinPath [
          rsync
          perl
          posixCommand
        ]
      }:$PATH"
    ''
  );
  passthru = { inherit gradle; };
  packages = [
    nodejs_26
    xmtp-pnpm
    python311
    jdk21
    jdk17
    gradle
    mitm-cache
    openssl
    python311.pkgs.ephemeral-port-reserve
  ]
  ++ lib.optionals pkgs.stdenv.isDarwin [
    cocoapods
    rsync
    perl
    posixCommand
  ];
  ANDROID_HOME = androidHome;
  ANDROID_SDK_ROOT = androidHome;
  ANDROID_NDK_HOME = "${androidHome}/ndk/${tools.android.ndk}";
  JAVA_HOME = jdk21.home;
  XMTP_RN_ANDROID_TOOLCHAIN = androidToolchain;
  XMTP_RN_GRADLE = "${gradle}/bin/gradle";
  XMTP_NIX_ENV = "yes";
  XMTP_RN_GRADLE_INIT_DEPS = "${path}/pkgs/development/tools/build-managers/gradle/init-deps.gradle";
  XMTP_RN_GRADLE_COMPRESS_DEPS = "${path}/pkgs/development/tools/build-managers/gradle/compress-deps-json.py";
}
