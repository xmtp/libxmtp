# Full local development shell — true superset of rust, android, and iOS shells.
# Includes a combined Rust toolchain with ALL cross-compilation targets,
# the Android SDK/emulator, iOS env setup (Darwin), debugging/profiling tools,
# and convenience packages.
# This is the `default` devShell (what you get with `nix develop`).
{
  stdenv,
  darwin,
  lib,
  mkShell,
  mktemp,
  jdk21,
  kotlin,
  ktlint,
  jdk17,
  diesel-cli,
  foundry-bin,
  sqlcipher,
  corepack,
  cargo-ndk,
  gnused,
  swiftformat,
  swiftlint,
  kotlin-language-server,
  xmtp,
  rust-analyzer,
  just,
}:
let
  inherit (stdenv) isDarwin;
  inherit (xmtp) shellCommon androidEnv iosEnv;

  # Combined Rust toolchain with ALL targets: wasm + linux + android + iOS (Darwin)
  allTargets = [
    "wasm32-unknown-unknown"
    "x86_64-unknown-linux-gnu"
  ]
  ++ androidEnv.androidTargets
  ++ lib.optionals isDarwin iosEnv.iosTargets;

  rust-toolchain = xmtp.mkToolchain allTargets [
    "rust-src"
    "clippy-preview"
    "rust-docs"
    "rustfmt-preview"
    "llvm-tools-preview"
  ];

in
mkShell {
  meta.description = "Full libXMTP local development environment";

  XMTP_DEV_SHELL = "local";

  # --- Rust base ---
  inherit (shellCommon.rustBase) hardeningDisable nativeBuildInputs LD_LIBRARY_PATH;
  inherit (shellCommon.rustBase.env)
    OPENSSL_DIR
    OPENSSL_LIB_DIR
    OPENSSL_NO_VENDOR
    STACK_OVERFLOW_CHECK
    XMTP_NIX_ENV
    ;
  inherit (shellCommon.wasmEnv)
    CC_wasm32_unknown_unknown
    AR_wasm32_unknown_unknown
    CFLAGS_wasm32_unknown_unknown
    ;

  # --- Android env vars ---
  ANDROID_HOME = androidEnv.devPaths.home;
  ANDROID_SDK_ROOT = androidEnv.devPaths.home;
  ANDROID_NDK_HOME = androidEnv.devPaths.ndkHome;
  ANDROID_NDK_ROOT = androidEnv.devPaths.ndkHome;
  NDK_HOME = androidEnv.devPaths.ndkHome;
  EMULATOR = "${androidEnv.emulator}";

  buildInputs =
    shellCommon.rustBase.buildInputs
    ++ [
      just
      # Combined toolchain (wasm + android + iOS targets)
      rust-toolchain
      rust-analyzer
      foundry-bin
      sqlcipher
      corepack

      # Android
      androidEnv.devComposition.androidsdk
      cargo-ndk
      androidEnv.emulator
      gnused

      # Kotlin / JDK
      jdk21
      kotlin
      ktlint
      jdk17
      kotlin-language-server

      # Misc dev
      mktemp
      diesel-cli
    ]
    # Wasm, cargo, CI, proto, lint tools
    ++ shellCommon.wasmTools
    ++ shellCommon.cargoTools
    ++ shellCommon.cargoCiTools
    ++ shellCommon.protoTools
    ++ shellCommon.lintTools
    # Debug & profiling
    ++ shellCommon.debugTools
    ++ shellCommon.miscDevTools
    # Darwin-specific
    ++ lib.optionals isDarwin [
      darwin.cctools
      swiftformat
      swiftlint
    ];

  shellHook = lib.optionalString isDarwin ''
    # --- iOS cross-compilation env setup ---
    # Unset SDKROOT so xcrun can discover the right SDK per target at build time.
    unset SDKROOT

    # Dynamically resolve Xcode path and set all cross-compilation env vars.
    ${iosEnv.envSetupAll}

    # Version validation — check that Xcode is recent enough for Swift 6.1 (Package Traits).
    XCODE_VERSION=$(xcodebuild -version 2>/dev/null | head -1 | awk '{print $2}')
    if [[ -n "$XCODE_VERSION" ]]; then
      MAJOR=$(echo "$XCODE_VERSION" | cut -d. -f1)
      if [[ "$MAJOR" -lt 16 ]]; then
        echo "WARNING: Xcode $XCODE_VERSION detected. Xcode 16+ required for Swift 6.1 (Package Traits)." >&2
      fi
    fi

    # Wrap `swift` to sanitize Nix compiler flags that conflict with SPM.
    # The local shell's large package set injects many -isystem/-L paths into
    # NIX_CFLAGS_COMPILE and NIX_LDFLAGS via Nix's cc-wrapper. Swift Package
    # Manager should use the Xcode toolchain exclusively, not Nix's paths.
    swift() {
      env \
        -u NIX_CFLAGS_COMPILE \
        -u NIX_CFLAGS_COMPILE_FOR_BUILD \
        -u NIX_LDFLAGS \
        -u NIX_LDFLAGS_FOR_BUILD \
        -u LD_LIBRARY_PATH \
        command swift "$@"
    }
  '';
}
