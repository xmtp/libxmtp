# iOS cross-compilation package derivation.
# Builds static and dynamic libraries for 4 targets + Swift bindings, cacheable in Cachix.
# Uses shared config from nix/lib/ios-env.nix and nix/lib/mobile-common.nix.
#
# This file produces 6 derivations:
#   1-4. Per-target static+dynamic libraries (impure — need Xcode SDK):
#        xmtpv3-{x86_64-apple-darwin,aarch64-apple-darwin,aarch64-apple-ios,aarch64-apple-ios-sim}
#   5.   Swift bindings (impure — needs Xcode SDK for native host build):
#        xmtpv3-swift-bindings (runs uniffi-bindgen, outputs .swift + .h + .modulemap)
#   6.   Aggregate (pure — just symlinks):
#        xmtpv3-ios-libs (combines all outputs into a single derivation)
#
# All impure derivations use __noChroot = true to access the system Xcode SDK.
# The aggregate derivation is pure since it only creates symlinks to Nix store paths.
#
# --- Why xcframework assembly stays in the Makefile ---
# It's technically feasible to move lipo + xcodebuild -create-xcframework into Nix
# (lipo is in the devShell, and __noChroot makes xcodebuild accessible). However:
#   1. Negligible caching benefit — lipo + xcodebuild takes ~5s vs 30-60 min compilation.
#   2. xcframework invalidates whenever any .a changes — so it's a cache miss exactly
#      when the static libs are also cache misses (the only scenario where caching matters).
#   3. Clean separation — Nix does expensive compilation/caching, Make does fast assembly.
#   4. `make local` would break — devs who don't use nix build depend on the Makefile flow.
#   5. Manually building xcframework (without xcodebuild) would be fragile.
{
  lib,
  xmtp,
  stdenv,
  ...
}:
let
  inherit (xmtp) iosEnv mobile;
  # override craneLib rust toolchain with given stdenv
  # https://crane.dev/API.html#mklib
  craneLib = xmtp.craneLib.overrideScope (
    _: _: {
      inherit stdenv;
    }
  );
  ffi-uniffi-bindgen = "${xmtp.ffi-uniffi-bindgen}/bin/ffi-uniffi-bindgen";
  # Rust toolchain with iOS/macOS cross-compilation targets.
  # No clippy/rustfmt — this is a build-only toolchain (the dev shell adds those).
  # overrideToolchain tells crane to use our custom fenix-based toolchain instead
  # of the default nixpkgs rustc.
  rust-toolchain = xmtp.mkToolchain iosEnv.iosTargets [ ];
  rust = craneLib.overrideToolchain (p: rust-toolchain);

  # Extract version once for use throughout the file
  version = mobile.mkVersion rust;

  # Inherit shared config
  inherit (mobile) commonArgs bindingsFileset;

  # Build static (.a) and dynamic (.dylib) libraries for a single cross-compilation target.
  #
  # Uses crane's two-phase approach:
  #   1. buildDepsOnly — compiles all dependencies, outputs cargo artifacts.
  #      Cache key: Cargo.lock + Cargo.toml files + build.rs files.
  #      This is the expensive step (~30-60 min) that benefits from Cachix.
  #   2. buildPackage — compiles project source using cached dep artifacts.
  #      Uses the full source fileset and inherits cargoArtifacts from phase 1.
  #
  # IMPROVEMENT: buildDepsOnly could include an Xcode existence check in its
  # buildPhaseCargoCommand to provide a clearer error message if Xcode is missing,
  # rather than failing deep in the cc-rs build with cryptic SDK path errors.
  buildTarget =
    target:
    let
      envSetup = iosEnv.envSetup target;

      # Phase 1: Dep caching — rebuilds when Cargo.lock, Cargo.toml, or build.rs change.
      cargoArtifacts = rust.buildDepsOnly (
        commonArgs
        // {
          pname = "xmtpv3-deps-${target}";
          CARGO_BUILD_TARGET = target;
          # Impure: needs Xcode SDK for bindgen during dep compilation
          __noChroot = true;
          cargoExtraArgs = "--target ${target} -p xmtpv3";
          # envSetup is inlined in buildPhaseCargoCommand because crane's buildDepsOnly
          # strips preBuild hooks (it needs full control of the build phase to replace
          # source files with dummies). envSetup dynamically resolves the Xcode path
          # via xcode-select and sets DEVELOPER_DIR, SDKROOT, CC/CXX, and bindgen args.
          buildPhaseCargoCommand = ''
            ${envSetup}
            cargo build --release --target ${target} -p xmtpv3
          '';
        }
      );
    in
    # Phase 2: Build project source using cached dep artifacts.
    rust.buildPackage (
      commonArgs
      // {
        inherit cargoArtifacts version;
        CARGO_BUILD_TARGET = target;
        __noChroot = true;
        pname = "xmtpv3-${target}";
        src = bindingsFileset;
        cargoExtraArgs = "--target ${target} -p xmtpv3";
        # preBuild works here (unlike buildDepsOnly) because buildPackage doesn't
        # need to replace source files, so crane leaves the build hooks intact.
        preBuild = iosEnv.envSetup target;
        # Override crane's default installPhase which expects a binary executable.
        # We produce a static library (.a) and dynamic library (.dylib), so we copy them directly.
        installPhaseCommand = ''
          mkdir -p $out/${target}
          cp target/${target}/release/libxmtpv3.a $out/${target}/
          cp target/${target}/release/libxmtpv3.dylib $out/${target}/
          install_name_tool -id @rpath/libxmtpv3.dylib $out/${target}/libxmtpv3.dylib
        '';
      }
    );

  # Native host env setup for Swift bindings (builds for macOS host, not iOS).
  nativeEnvSetup = iosEnv.envSetup "aarch64-apple-darwin";

  # Swift bindings derivation.
  # Builds libxmtpv3 for the native macOS host, then runs uniffi-bindgen to
  # generate Swift bindings (.swift file + C header + modulemap).
  # Impure because it needs the Xcode SDK for the native host build.
  # The generated files are platform-independent — they work with any target's .a file.
  swiftBindings = rust.buildPackage (
    commonArgs
    // {
      pname = "xmtpv3-swift-bindings";
      __noChroot = true;
      src = bindingsFileset;
      inherit version;
      cargoArtifacts = rust.buildDepsOnly (
        commonArgs
        // {
          pname = "xmtpv3-swift-bindings-deps";
          __noChroot = true;
          cargoExtraArgs = "-p xmtpv3";
          buildPhaseCargoCommand = ''
            ${nativeEnvSetup}
            cargo build --release -p xmtpv3
          '';
        }
      );
      cargoExtraArgs = "-p xmtpv3";
      buildPhaseCargoCommand = ''
        ${nativeEnvSetup}
        cargo build --release -p xmtpv3
      '';
      # Prevent crane from trying to find and install cargo binaries from $out/bin.
      # This derivation produces generated source files, not executables.
      doNotPostBuildInstallCargoBinaries = true;
      installPhaseCommand = ''
        ${nativeEnvSetup}
        # Generate Swift bindings using uniffi-bindgen.
        # This runs the ffi-uniffi-bindgen binary (built above) against the compiled
        # static library to extract the FFI interface and produce:
        #   - xmtpv3.swift: Swift source with all public API types and functions
        #   - xmtpv3FFI.h: C header for the FFI layer
        #   - xmtpv3FFI.modulemap: Clang module map (renamed to module.modulemap)
        ${ffi-uniffi-bindgen} generate \
          --library target/release/libxmtpv3.a \
          --out-dir $TMPDIR/swift-out \
          --language swift

        # Organize into expected directory structure for xcframework assembly
        mkdir -p $out/swift/include/libxmtp
        cp $TMPDIR/swift-out/xmtpv3.swift $out/swift/
        mv $TMPDIR/swift-out/xmtpv3FFI.h $out/swift/include/libxmtp/
        mv $TMPDIR/swift-out/xmtpv3FFI.modulemap $out/swift/include/libxmtp/module.modulemap
      '';
    }
  );

  # Function to build a specific set of targets (mirrors mkAndroid in android.nix)
  mkIos =
    targetList:
    let
      selectedTargets = lib.genAttrs targetList buildTarget;

      selectedAggregate = stdenv.mkDerivation {
        pname = "xmtpv3-ios-libs";
        inherit version;
        dontUnpack = true;
        installPhase = ''
          mkdir -p $out/swift
          ${lib.concatMapStringsSep "\n" (target: ''
            mkdir -p $out/${target}
            ln -s ${selectedTargets.${target}}/${target}/libxmtpv3.a $out/${target}/libxmtpv3.a
            ln -s ${selectedTargets.${target}}/${target}/libxmtpv3.dylib $out/${target}/libxmtpv3.dylib
          '') targetList}
          ln -s ${swiftBindings}/swift/xmtpv3.swift $out/swift/xmtpv3.swift
          ln -s ${swiftBindings}/swift/include $out/swift/include
        '';
      };
    in
    {
      targets = selectedTargets;
      inherit swiftBindings;
      aggregate = selectedAggregate;
    };

in
{
  inherit swiftBindings mkIos;
  # Default: all targets (for backward compat)
  inherit (mkIos iosEnv.iosTargets) targets aggregate;
}
