# iOS cross-compilation dev shell.
# Uses shared config from nix/lib/ios-env.nix.
{ stdenv
, darwin
, lib
, pkg-config
, mkShell
, openssl
, sqlite
, zstd
, xmtp
, swiftformat
, swiftlint
, ...
}:

let
  inherit (stdenv) isDarwin;
  iosEnv = import ./lib/ios-env.nix { inherit lib; };
  rust-ios-toolchain = xmtp.mkToolchain iosEnv.iosTargets [ "clippy-preview" "rustfmt-preview" ];
in
mkShell {
  hardeningDisable = [ "zerocallusedregs" ];

  nativeBuildInputs = [ pkg-config ];
  buildInputs =
    [
      rust-ios-toolchain
      zstd
      openssl
      sqlite
      swiftformat
      swiftlint
    ]
    ++ lib.optionals isDarwin [
      darwin.cctools
    ];

  shellHook = ''
    # Override Nix's apple-sdk DEVELOPER_DIR with real Xcode path.
    # Must be in shellHook because Nix's setup hooks set DEVELOPER_DIR after env attrs.
    export DEVELOPER_DIR="${iosEnv.developerDir}"

    # Unset SDKROOT so xcrun can discover the right SDK per target at build time.
    # (The package derivation sets SDKROOT per-target; the shell leaves it to xcrun.)
    unset SDKROOT

    # Use Xcode toolchain clang for iOS cross-compilation.
    # NOT /usr/bin/clang, which is an xcode-select shim that reads DEVELOPER_DIR.
    export CC_aarch64_apple_ios="${iosEnv.xcodeClang}"
    export CXX_aarch64_apple_ios="${iosEnv.xcodeClangxx}"
    export CC_aarch64_apple_ios_sim="${iosEnv.xcodeClang}"
    export CXX_aarch64_apple_ios_sim="${iosEnv.xcodeClangxx}"

    export CARGO_TARGET_AARCH64_APPLE_IOS_LINKER="${iosEnv.xcodeClang}"
    export CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER="${iosEnv.xcodeClang}"

    export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios="--target=arm64-apple-ios --sysroot=${iosEnv.iosSdk}"
    export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios_sim="--target=arm64-apple-ios-simulator --sysroot=${iosEnv.iosSimSdk}"

    if [[ ! -d "${iosEnv.developerDir}" ]]; then
      echo "ERROR: Xcode not found at ${iosEnv.developerDir}" >&2
      echo "iOS builds require Xcode. Install from App Store or run:" >&2
      echo "  xcode-select --install" >&2
      echo "  sudo xcode-select -s /Applications/Xcode.app/Contents/Developer" >&2
      return 1
    fi

    # Prepend Xcode's bin to PATH so system xcodebuild/xcrun are used
    export PATH="${iosEnv.developerDir}/usr/bin:$PATH"
  '';
}
