# iOS cross-compilation dev shell.
# Provides the environment for `cargo build --target <ios-target>` from the CLI.
# Uses shared config from nix/lib/ios-env.nix.
#
# Relationship to nix/package/ios.nix:
#   - This file: interactive dev shell (`nix develop .#ios`)
#   - package/ios.nix: CI/release build derivation (`nix build .#ios-libs`)
# Both use ios-env.nix for shared cross-compilation config.
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

  # Rust toolchain with all iOS/macOS cross-compilation targets.
  # Includes clippy and rustfmt for dev use (the package derivation omits these
  # since it only needs to compile, not lint).
  rust-ios-toolchain = xmtp.mkToolchain iosEnv.iosTargets [ "clippy-preview" "rustfmt-preview" ];
in
mkShell {
  # zerocallusedregs is a hardening flag that Nix enables by default.
  # It uses a calling convention that Xcode's clang doesn't support,
  # causing "unknown flag" errors during iOS cross-compilation.
  hardeningDisable = [ "zerocallusedregs" ];

  nativeBuildInputs = [ pkg-config ];
  buildInputs =
    [
      rust-ios-toolchain
      zstd
      openssl
      sqlite
      # Swift code formatting/linting tools for the iOS SDK development
      swiftformat
      swiftlint
    ]
    ++ lib.optionals isDarwin [
      # cctools provides lipo for combining multi-arch static libraries
      # into universal (fat) binaries in the Makefile's `lipo` target.
      darwin.cctools
    ];

  shellHook = ''
    export XMTP_DEV_SHELL="ios"

    # Unset SDKROOT so xcrun can discover the right SDK per target at build time.
    # (The package derivation sets SDKROOT per-target; the shell leaves it to xcrun.)
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
  '';
}
