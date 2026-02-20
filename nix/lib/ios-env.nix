# Shared iOS cross-compilation environment configuration.
# Used by both nix/ios.nix (dev shell) and nix/package/ios.nix (build derivation).
#
# All Xcode paths are resolved dynamically at shell time via /usr/bin/xcode-select.
# This ensures CI runners using setup-xcode (which installs to versioned paths like
# /Applications/Xcode_26.1.1.app) get the correct toolchain automatically.
#
# Key insight: /usr/bin/clang is an xcode-select shim that reads DEVELOPER_DIR.
# Nix's stdenv overrides DEVELOPER_DIR to its own apple-sdk, causing the shim
# to dispatch to Nix's cc-wrapper (which injects -mmacos-version-min, breaking
# iOS builds). We bypass this by using the full Xcode toolchain clang path,
# resolved dynamically from the active Xcode installation.
{ lib }:
let
  # Cross-compilation targets for the iOS release:
  #   x86_64-apple-darwin    — macOS Intel (for universal macOS binary)
  #   aarch64-apple-darwin   — macOS Apple Silicon (for universal macOS binary)
  #   aarch64-apple-ios      — iOS device (arm64)
  #   aarch64-apple-ios-sim  — iOS simulator on Apple Silicon
  iosTargets = [
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
    "aarch64-apple-ios"
    "aarch64-apple-ios-sim"
  ];

  # Default Xcode path — used as fallback when /usr/bin/xcode-select is unavailable.
  defaultDeveloperDir = "/Applications/Xcode.app/Contents/Developer";

  # iOS targets need explicit CC/CXX overrides to bypass Nix's cc-wrapper,
  # which injects macOS-specific flags (e.g., -mmacos-version-min) that break
  # iOS compilation. macOS targets don't need this — Nix's cc-wrapper is
  # compatible with macOS builds.
  isIosTarget =
    target:
    builtins.elem target [
      "aarch64-apple-ios"
      "aarch64-apple-ios-sim"
    ];

  # SDK path suffix for a given target (relative to DEVELOPER_DIR).
  sdkSuffixForTarget =
    target:
    {
      "aarch64-apple-ios" = "Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk";
      "aarch64-apple-ios-sim" = "Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk";
      "x86_64-apple-darwin" = "Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk";
      "aarch64-apple-darwin" = "Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk";
    }
    .${target};

  # Shell snippet that resolves the active Xcode installation path.
  # Prefers /usr/bin/xcode-select (respects setup-xcode on CI), falls back to default.
  # Sets _XCODE_DEV for use by subsequent shell snippets.
  #
  # Inside Nix build sandboxes, xcode-select -p returns a Nix store path
  # (e.g., /nix/store/...-apple-sdk-14.4) which is NOT a real Xcode installation.
  # We detect this by checking if the result starts with /nix/ and skip it.
  resolveXcode = ''
    _XCODE_DEV="${defaultDeveloperDir}"
    # Unset DEVELOPER_DIR before querying xcode-select, because:
    #   1. Nix's apple-sdk sets DEVELOPER_DIR to a Nix store path
    #   2. xcode-select -p honors DEVELOPER_DIR over the system setting
    #   3. We want the system setting (set by setup-xcode on CI)
    if DEVELOPER_DIR= /usr/bin/xcode-select -p &>/dev/null; then
      _XCODE_SELECT=$(DEVELOPER_DIR= /usr/bin/xcode-select -p)
      # Skip Nix store paths — inside build sandboxes, xcode-select returns
      # a Nix apple-sdk path that lacks the real Xcode toolchain.
      if [[ "$_XCODE_SELECT" != /nix/* ]]; then
        _XCODE_DEV="$_XCODE_SELECT"
      fi
    fi
  '';

  # Shell snippet that sets all Xcode-derived environment variables for a specific
  # cross-compilation target. Must run as shell code because:
  #   1. Xcode path is resolved dynamically (CI vs local may differ)
  #   2. Nix's apple-sdk setup hook sets DEVELOPER_DIR/SDKROOT AFTER derivation
  #      env var assignment, so we must override them at shell time
  #
  # For iOS targets, also bypasses Nix's cc-wrapper which injects conflicting
  # macOS flags (e.g., -mmacos-version-min).
  #
  # In buildDepsOnly: inline in buildPhaseCargoCommand (preBuild is stripped by crane).
  # In buildPackage: use as preBuild hook.
  envSetup =
    target:
    resolveXcode
    + ''
      export DEVELOPER_DIR="$_XCODE_DEV"
      export SDKROOT="$_XCODE_DEV/${sdkSuffixForTarget target}"
      export IPHONEOS_DEPLOYMENT_TARGET="14"
      export PATH="$_XCODE_DEV/usr/bin:$PATH"
    ''
    + lib.optionalString (isIosTarget target) ''
      _XCODE_CLANG="$_XCODE_DEV/Toolchains/XcodeDefault.xctoolchain/usr/bin/clang"
      _XCODE_CLANGXX="$_XCODE_DEV/Toolchains/XcodeDefault.xctoolchain/usr/bin/clang++"
      export CC="$_XCODE_CLANG"
      export CXX="$_XCODE_CLANGXX"
    ''
    + lib.optionalString (target == "aarch64-apple-ios") ''
      export CC_aarch64_apple_ios="$_XCODE_CLANG"
      export CXX_aarch64_apple_ios="$_XCODE_CLANGXX"
      export CARGO_TARGET_AARCH64_APPLE_IOS_LINKER="$_XCODE_CLANG"
      export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios="--target=arm64-apple-ios --sysroot=$SDKROOT"
    ''
    + lib.optionalString (target == "aarch64-apple-ios-sim") ''
      export CC_aarch64_apple_ios_sim="$_XCODE_CLANG"
      export CXX_aarch64_apple_ios_sim="$_XCODE_CLANGXX"
      export CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER="$_XCODE_CLANG"
      export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios_sim="--target=arm64-apple-ios-simulator --sysroot=$SDKROOT"
    '';

  # Shell snippet that sets all cross-compilation env vars for the dev shell.
  # Unlike envSetup (which targets a single platform), this configures env vars
  # for all iOS targets at once. SDKROOT is left unset so xcrun can discover
  # the right SDK per invocation.
  envSetupAll = resolveXcode + ''
    if [[ ! -d "$_XCODE_DEV" ]]; then
      echo "ERROR: Xcode not found at $_XCODE_DEV" >&2
      echo "iOS builds require Xcode. Install from App Store or run:" >&2
      echo "  xcode-select --install" >&2
      echo "  sudo xcode-select -s /Applications/Xcode.app/Contents/Developer" >&2
      return 1
    fi

    export DEVELOPER_DIR="$_XCODE_DEV"
    export IPHONEOS_DEPLOYMENT_TARGET="14"
    _XCODE_CLANG="$_XCODE_DEV/Toolchains/XcodeDefault.xctoolchain/usr/bin/clang"
    _XCODE_CLANGXX="$_XCODE_DEV/Toolchains/XcodeDefault.xctoolchain/usr/bin/clang++"
    _IOS_SDK="$_XCODE_DEV/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk"
    _IOS_SIM_SDK="$_XCODE_DEV/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk"
    export CC_aarch64_apple_ios="$_XCODE_CLANG"
    export CXX_aarch64_apple_ios="$_XCODE_CLANGXX"
    export CC_aarch64_apple_ios_sim="$_XCODE_CLANG"
    export CXX_aarch64_apple_ios_sim="$_XCODE_CLANGXX"
    export CARGO_TARGET_AARCH64_APPLE_IOS_LINKER="$_XCODE_CLANG"
    export CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER="$_XCODE_CLANG"
    export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios="--target=arm64-apple-ios --sysroot=$_IOS_SDK"
    export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios_sim="--target=arm64-apple-ios-simulator --sysroot=$_IOS_SIM_SDK"
    export PATH="$_XCODE_DEV/usr/bin:$PATH"
  '';

in
{
  inherit
    iosTargets
    defaultDeveloperDir
    isIosTarget
    envSetup
    envSetupAll
    ;
}
