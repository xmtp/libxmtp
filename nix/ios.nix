# This build will only work on darwin
{ stdenv
, darwin
, lib
, pkg-config
, mkShell
, openssl
, sqlite
, zstd
, llvmPackages_19
, xcbuild
, xmtp
, swiftformat
, swiftlint
, ...
}:

let
  inherit (stdenv) isDarwin;

  # Note: x86_64-apple-ios (Intel simulator) dropped - Apple Silicon is standard now
  iosTargets = [
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
    "aarch64-apple-ios"
    "aarch64-apple-ios-sim"
  ];

  # Pinned Rust Version
  rust-ios-toolchain = xmtp.mkToolchain iosTargets [ "clippy-preview" "rustfmt-preview" ];
in
mkShell {
  # For iOS cross-compilation, we use vendored openssl (builds from source for each target)
  # The Nix-provided openssl is macOS-only and can't be linked into iOS binaries
  # OPENSSL_NO_VENDOR is intentionally NOT set for iOS builds

  LLVM_PATH = "${llvmPackages_19.stdenv}";
  hardeningDisable = [ "zerocallusedregs" ];

  nativeBuildInputs = [ pkg-config ];
  buildInputs =
    [
      rust-ios-toolchain

      # native libs
      zstd
      openssl
      sqlite
      xcbuild

      # Swift tooling
      swiftformat
      swiftlint
    ]
    ++ lib.optionals isDarwin [
      darwin.cctools
    ];

  shellHook = ''
    # Use system Xcode for iOS SDK access (iPhoneOS, iPhoneSimulator platforms)
    # The Nix apple-sdk only includes MacOSX.platform, so we override at runtime
    export DEVELOPER_DIR="/Applications/Xcode.app/Contents/Developer"

    # Unset SDKROOT so xcrun can discover the right SDK per target
    # This is critical for iOS cross-compilation where different targets
    # need different SDKs (iphoneos vs iphonesimulator vs macosx)
    unset SDKROOT

    # Use system clang for iOS cross-compilation targets
    # Nix's cc-wrapper adds macOS-specific flags that conflict with iOS builds
    export CC_aarch64_apple_ios="/usr/bin/clang"
    export CXX_aarch64_apple_ios="/usr/bin/clang++"
    export CC_aarch64_apple_ios_sim="/usr/bin/clang"
    export CXX_aarch64_apple_ios_sim="/usr/bin/clang++"

    # Use system linker for iOS targets to avoid Nix's macOS-only libraries (libiconv, etc.)
    export CARGO_TARGET_AARCH64_APPLE_IOS_LINKER="/usr/bin/clang"
    export CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER="/usr/bin/clang"

    # Set up SDK paths for bindgen (used by tracing-oslog and other crates)
    IOS_SDK="$DEVELOPER_DIR/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk"
    IOS_SIM_SDK="$DEVELOPER_DIR/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk"

    # Bindgen needs --target and --sysroot for cross-compilation
    # The target triple for clang differs from Rust's target triple for simulators
    export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios="--target=arm64-apple-ios --sysroot=$IOS_SDK"
    export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios_sim="--target=arm64-apple-ios-simulator --sysroot=$IOS_SIM_SDK"

    if [[ ! -d "$DEVELOPER_DIR" ]]; then
      echo "ERROR: Xcode not found at $DEVELOPER_DIR" >&2
      echo "iOS builds require Xcode. Install from App Store or run:" >&2
      echo "  xcode-select --install" >&2
      echo "  sudo xcode-select -s /Applications/Xcode.app/Contents/Developer" >&2
      return 1
    fi

    # Prepend Xcode's bin to PATH so system xcodebuild is used (Nix's xcbuild doesn't support -create-xcframework)
    export PATH="$DEVELOPER_DIR/usr/bin:$PATH"
  '';
}
