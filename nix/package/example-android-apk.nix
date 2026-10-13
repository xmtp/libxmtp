{
  lib,
  stdenvNoCC,
  gradle-packages,
  jdk17,
  python311,
  openssl,
  fetchurl,
  xmtp,
  androidLibraries,
  generatedKotlin,
}:
let
  app = ../../apps/example-android;
  sdk = ../../sdks/android;
  versionLine = lib.findFirst (lib.hasPrefix "version=") null (
    lib.splitString "\n" (builtins.readFile (sdk + "/gradle.properties"))
  );
  gradle =
    (gradle-packages.mkGradle {
      version = "8.11.1";
      hash = "sha256-85eyhwI6zboen2/F6nLSLdY2adWe1KKJopsadu7hUcY=";
      defaultJava = jdk17;
    }).wrapped;
  # AOSP publishes this key for testing. It is not a private release credential.
  testKey = fetchurl {
    url = "https://android.googlesource.com/platform/build/+/android-15.0.0_r1/target/product/security/testkey.pk8?format=TEXT";
    sha256 = "91d728fe501694798975078002f8fa9aa7dd23b4591015401b63b7d06ac528e9";
  };
  testCertificate = fetchurl {
    url = "https://android.googlesource.com/platform/build/+/android-15.0.0_r1/target/product/security/testkey.x509.pem?format=TEXT";
    sha256 = "be3a20553271524b7f0dd5cb41c3c2fa2caa41fc5e18fa3f3806d497dd820ed3";
  };
  configFiles = lib.fileset.unions (
    map
      (
        directory:
        lib.fileset.fileFilter (
          file:
          file.hasExt "gradle"
          || file.hasExt "properties"
          || file.hasExt "lockfile"
          || file.hasExt "pro"
          || file.name == "verification-metadata.xml"
        ) directory
      )
      [
        app
        sdk
      ]
  );
in
assert lib.hasInfix "gradle-${gradle.version}-bin.zip" (
  builtins.readFile (sdk + "/gradle/wrapper/gradle-wrapper.properties")
);
stdenvNoCC.mkDerivation (finalAttrs: {
  pname = "xmtp-messenger-apk";
  version = lib.removePrefix "version=" versionLine;

  src = lib.fileset.toSource {
    root = ../..;
    fileset = lib.fileset.unions [
      configFiles
      (app + "/app/src/main")
      (app + "/app/src/pushOff")
      (app + "/shared/src/commonMain")
      (app + "/shared/src/androidMain")
      (sdk + "/library/src/main")
    ];
  };

  nativeBuildInputs = [
    gradle
    python311
    openssl
  ];
  mitmCache = gradle.fetchDeps {
    pkg = finalAttrs.finalPackage;
    data = ./example-android-apk-deps.json;
  };

  # Gradle and the dependency replay proxy need loopback, not external network access.
  __darwinAllowLocalNetworking = true;
  # Darwin's sandbox rejects Java's IPv6-mapped loopback sockets.
  JAVA_TOOL_OPTIONS = "-Djava.net.preferIPv4Stack=true";
  ANDROID_HOME = xmtp.androidEnv.buildPaths.home;
  ANDROID_SDK_ROOT = xmtp.androidEnv.buildPaths.home;
  ANDROID_NDK_HOME = xmtp.androidEnv.buildPaths.ndkHome;
  XMTP_SDK_GENERATED_DIR = generatedKotlin;
  XMTP_SDK_ANDROID_JNI_DIR = "${androidLibraries}/jniLibs";
  # A USB-connected device can reach this with adb reverse; remote servers use HTTPS.
  XMTP_ANDROID_BACKEND_URL = "http://127.0.0.1:5050";

  preConfigure = ''
    export HOME="$TMPDIR/home"
    export ANDROID_USER_HOME="$HOME/.android"
    export JAVA_TOOL_OPTIONS="$JAVA_TOOL_OPTIONS -Duser.home=$HOME"
    mkdir -p "$ANDROID_USER_HOME"
    base64 --decode ${testKey} > "$HOME/testing.pk8"
    base64 --decode ${testCertificate} > "$HOME/testing.crt"
    openssl pkcs8 -inform DER -in "$HOME/testing.pk8" -nocrypt -out "$HOME/testing.pem"
    openssl pkcs12 -export -inkey "$HOME/testing.pem" -in "$HOME/testing.crt" \
      -name xmtp-testing -passout pass:android -out "$HOME/testing.p12"
    gradleFlagsArray+=(
      "-Pandroid.injected.signing.store.file=$HOME/testing.p12"
      "-Pandroid.injected.signing.store.password=android"
      "-Pandroid.injected.signing.key.alias=xmtp-testing"
      "-Pandroid.injected.signing.key.password=android"
    )
    cd apps/example-android
  '';
  gradleFlags = [
    "--dependency-verification=strict"
    "-PxmtpExampleVersion=${finalAttrs.version}"
    "-Dorg.gradle.java.home=${jdk17}"
    "-Pkotlin.compiler.execution.strategy=in-process"
    "-Pandroid.aapt2FromMavenOverride=${xmtp.androidEnv.buildPaths.home}/build-tools/35.0.0/aapt2"
  ];
  gradleBuildTask = ":example:assembleRelease";
  gradleUpdateTask = finalAttrs.gradleBuildTask;

  installPhase = ''
    runHook preInstall
    apk=app/build/outputs/apk/release/example-release.apk
    python3.11 -B ${../../sdks/android/dev/check-native-packages.py} --require-all-abis "$apk"
    "$ANDROID_HOME/build-tools/35.0.0/apksigner" verify "$apk"
    "$ANDROID_HOME/build-tools/35.0.0/aapt" dump badging "$apk" > "$TMPDIR/apk-badging"
    if grep -q application-debuggable "$TMPDIR/apk-badging"; then
      echo "The downloadable APK must use the non-debuggable release variant." >&2
      exit 1
    fi
    mkdir -p "$out"
    cp "$apk" "$out/xmtp-messenger.apk"
    printf '%s\n' '${finalAttrs.version}' > "$out/version"
    runHook postInstall
  '';
  dontFixup = true;
  meta = {
    description = "Universal XMTP Messenger Android testing APK";
    platforms = [
      "aarch64-darwin"
      "x86_64-linux"
    ];
    license = lib.licenses.mit;
  };
})
