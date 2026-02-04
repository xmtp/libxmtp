# iOS cross-compilation package derivation.
# Builds static libraries for 4 targets + Swift bindings, cacheable in Cachix.
# Uses shared config from nix/lib/ios-env.nix.
#
# This file produces 6 derivations:
#   1-4. Per-target static libraries (impure — need Xcode SDK):
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
{ lib
, fenix
, zstd
, openssl
, sqlite
, pkg-config
, perl
, craneLib
, xmtp
, stdenv
, ...
}:
let
  iosEnv = import ./../lib/ios-env.nix { inherit lib; };

  # Rust toolchain with iOS/macOS cross-compilation targets.
  # No clippy/rustfmt — this is a build-only toolchain (the dev shell adds those).
  # overrideToolchain tells crane to use our custom fenix-based toolchain instead
  # of the default nixpkgs rustc.
  rust-toolchain = xmtp.mkToolchain iosEnv.iosTargets [];
  rust = craneLib.overrideToolchain (p: rust-toolchain);

  # Narrow fileset for buildDepsOnly — only includes files that affect
  # dependency compilation. Cargo.toml/Cargo.lock for resolution, build.rs
  # for build scripts, plus files referenced by build scripts.
  # Source (.rs) changes don't invalidate the dep cache since crane replaces
  # them with dummies anyway.
  #
  # If a new crate with a build.rs is added to the workspace, its build.rs
  # will be picked up automatically by the fileFilter below. If that build.rs
  # references additional files (like JSON configs or migration dirs), those
  # files must be added to the union list manually.
  depsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = lib.fileset.unions [
      ../../Cargo.lock
      ../../.cargo/config.toml
      # All Cargo.toml and build.rs files in the workspace
      (lib.fileset.fileFilter (file:
        file.name == "Cargo.toml" || file.name == "build.rs"
      ) ../../.)
      # Files referenced by build scripts (e.g., include_bytes!, include_str!).
      # These are needed at dep-compilation time because build.rs runs then.
      ../../crates/xmtp_id/src/scw_verifier/chain_urls_default.json
      ../../crates/xmtp_id/artifact
      ../../crates/xmtp_id/src/scw_verifier/signature_validation.hex
      ../../crates/xmtp_db/migrations
      ../../crates/xmtp_proto/src/gen/proto_descriptor.bin
    ];
  };

  # Full fileset for buildPackage — includes all source files needed to compile
  # the xmtpv3 crate and its workspace dependencies.
  # Uses xmtp.filesets.forCrate which walks Cargo.toml dependencies to include
  # only relevant crates, unlike depsFileset which excludes .rs files entirely.
  bindingsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = (xmtp.filesets { inherit lib craneLib; }).forCrate ./../../bindings/mobile;
  };

  commonArgs = {
    # depsFileset for buildDepsOnly; buildPackage calls override with bindingsFileset
    src = depsFileset;
    strictDeps = true;
    # perl is needed for openssl-sys's vendored build (its Configure script is Perl).
    nativeBuildInputs = [ pkg-config perl ];
    buildInputs = [ zstd openssl sqlite ];
    doCheck = false;
    # For iOS cross-compilation, openssl must be vendored (built from source per target).
    # Do NOT set OPENSSL_NO_VENDOR — that would try to link a macOS-built libssl
    # into an iOS binary, causing linker errors.
    hardeningDisable = [ "zerocallusedregs" ];
  };

  # Build a static library (.a) for a single cross-compilation target.
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
  buildTarget = target:
    let
      targetEnv = iosEnv.envVars // {
        CARGO_BUILD_TARGET = target;
      };

      envSetup = iosEnv.envSetup target;

      # Phase 1: Dep caching — rebuilds when Cargo.lock, Cargo.toml, or build.rs change.
      cargoArtifacts = rust.buildDepsOnly (targetEnv // commonArgs // {
        pname = "xmtpv3-deps-${target}";
        # Impure: needs Xcode SDK for bindgen during dep compilation
        __noChroot = true;
        cargoExtraArgs = "--target ${target} -p xmtpv3";
        # envSetup is inlined in buildPhaseCargoCommand because crane's buildDepsOnly
        # strips preBuild hooks (it needs full control of the build phase to replace
        # source files with dummies). The env overrides must happen here to counteract
        # Nix's apple-sdk setup hook which sets DEVELOPER_DIR/SDKROOT after derivation
        # env vars are applied.
        buildPhaseCargoCommand = ''
          ${envSetup}
          cargo build --release --target ${target} -p xmtpv3
        '';
      });
    in
    # Phase 2: Build project source using cached dep artifacts.
    rust.buildPackage (targetEnv // commonArgs // {
      inherit cargoArtifacts;
      __noChroot = true;
      pname = "xmtpv3-${target}";
      src = bindingsFileset;
      inherit (rust.crateNameFromCargoToml {
        cargoToml = ./../../Cargo.toml;
      }) version;
      cargoExtraArgs = "--target ${target} -p xmtpv3";
      # preBuild works here (unlike buildDepsOnly) because buildPackage doesn't
      # need to replace source files, so crane leaves the build hooks intact.
      preBuild = iosEnv.envSetup target;
      # Override crane's default installPhase which expects a binary executable.
      # We produce a static library (.a), so we copy it directly.
      installPhaseCommand = ''
        mkdir -p $out/${target}
        cp target/${target}/release/libxmtpv3.a $out/${target}/
      '';
    });

  # Per-target derivations (genAttrs creates { "x86_64-apple-darwin" = <drv>; ... })
  targets = lib.genAttrs iosEnv.iosTargets buildTarget;

  # Native host env setup for Swift bindings (builds for macOS host, not iOS).
  nativeEnvSetup = iosEnv.envSetup "aarch64-apple-darwin";

  # Swift bindings derivation.
  # Builds libxmtpv3 for the native macOS host, then runs uniffi-bindgen to
  # generate Swift bindings (.swift file + C header + modulemap).
  # Impure because it needs the Xcode SDK for the native host build.
  # The generated files are platform-independent — they work with any target's .a file.
  swiftBindings = rust.buildPackage (commonArgs // {
    pname = "xmtpv3-swift-bindings";
    __noChroot = true;
    src = bindingsFileset;
    inherit (rust.crateNameFromCargoToml {
      cargoToml = ./../../Cargo.toml;
    }) version;
    cargoArtifacts = rust.buildDepsOnly (commonArgs // {
      pname = "xmtpv3-swift-bindings-deps";
      __noChroot = true;
      cargoExtraArgs = "-p xmtpv3";
      buildPhaseCargoCommand = ''
        ${nativeEnvSetup}
        cargo build --release -p xmtpv3
      '';
    });
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
      cargo run -p xmtpv3 --bin ffi-uniffi-bindgen --release --features uniffi/cli generate \
        --library target/release/libxmtpv3.a \
        --out-dir $TMPDIR/swift-out \
        --language swift

      # Organize into expected directory structure for xcframework assembly
      mkdir -p $out/swift/include/libxmtp
      cp $TMPDIR/swift-out/xmtpv3.swift $out/swift/
      mv $TMPDIR/swift-out/xmtpv3FFI.h $out/swift/include/libxmtp/
      mv $TMPDIR/swift-out/xmtpv3FFI.modulemap $out/swift/include/libxmtp/module.modulemap
    '';
  });

  # Aggregate derivation: combines all per-target static libraries + Swift bindings
  # into a single output directory.
  # Uses symlinks instead of copies to avoid ~100MB duplication in the Nix store
  # (each .a file is 20-30MB). The Makefile's lipo/framework targets follow symlinks.
  # dontUnpack = true because there's no source to extract — this derivation only
  # creates symlinks to other Nix store paths.
  aggregate = stdenv.mkDerivation {
    pname = "xmtpv3-ios-libs";
    version = (rust.crateNameFromCargoToml {
      cargoToml = ./../../Cargo.toml;
    }).version;
    dontUnpack = true;
    installPhase = ''
      mkdir -p $out/swift
      ${lib.concatMapStringsSep "\n" (target: ''
        mkdir -p $out/${target}
        ln -s ${targets.${target}}/${target}/libxmtpv3.a $out/${target}/libxmtpv3.a
      '') iosEnv.iosTargets}
      ln -s ${swiftBindings}/swift/xmtpv3.swift $out/swift/xmtpv3.swift
      ln -s ${swiftBindings}/swift/include $out/swift/include
    '';
  };

in
{
  inherit targets swiftBindings aggregate;
}
