# iOS cross-compilation package derivation.
# Builds static libraries for 4 targets + Swift bindings, cacheable in Cachix.
# Uses shared config from nix/lib/ios-env.nix.
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

  # Pinned Rust Version with iOS targets
  rust-toolchain = xmtp.mkToolchain iosEnv.iosTargets [];
  rust = craneLib.overrideToolchain (p: rust-toolchain);

  # Narrow fileset for buildDepsOnly — only includes files that affect
  # dependency compilation. Cargo.toml/Cargo.lock for resolution, build.rs
  # for build scripts, plus files referenced by build scripts.
  # Source (.rs) changes don't invalidate the dep cache since crane replaces
  # them with dummies anyway.
  depsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = lib.fileset.unions [
      ../../Cargo.lock
      ../../.cargo/config.toml
      # All Cargo.toml and build.rs files in the workspace
      (lib.fileset.fileFilter (file:
        file.name == "Cargo.toml" || file.name == "build.rs"
      ) ../../.)
      # Files referenced by build scripts
      ../../crates/xmtp_id/src/scw_verifier/chain_urls_default.json
      ../../crates/xmtp_id/artifact
      ../../crates/xmtp_id/src/scw_verifier/signature_validation.hex
      ../../crates/xmtp_db/migrations
      ../../crates/xmtp_proto/src/gen/proto_descriptor.bin
    ];
  };

  bindingsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = (xmtp.filesets { inherit lib craneLib; }).forCrate ./../../bindings/mobile;
  };

  commonArgs = {
    # depsFileset for buildDepsOnly; buildPackage calls override with bindingsFileset
    src = depsFileset;
    strictDeps = true;
    # perl is needed for openssl-sys's vendored build (Configure script)
    nativeBuildInputs = [ pkg-config perl ];
    buildInputs = [ zstd openssl sqlite ];
    doCheck = false;
    # For iOS cross-compilation, openssl must be vendored (builds from source per target)
    # Do NOT set OPENSSL_NO_VENDOR
    hardeningDisable = [ "zerocallusedregs" ];
  };

  # Build a static library for a single target
  buildTarget = target:
    let
      targetEnv = iosEnv.envVars // {
        CARGO_BUILD_TARGET = target;
      };

      envSetup = iosEnv.envSetup target;

      # Dep caching: rebuilds when Cargo.lock, Cargo.toml, or build.rs change
      cargoArtifacts = rust.buildDepsOnly (targetEnv // commonArgs // {
        pname = "xmtpv3-deps-${target}";
        # Impure: needs Xcode SDK for bindgen during dep compilation
        __noChroot = true;
        cargoExtraArgs = "--target ${target} -p xmtpv3";
        # Inline env setup in the build command because crane's buildDepsOnly
        # strips preBuild hooks, and Nix's apple-sdk overrides env vars.
        buildPhaseCargoCommand = ''
          ${envSetup}
          cargo build --release --target ${target} -p xmtpv3
        '';
      });
    in
    rust.buildPackage (targetEnv // commonArgs // {
      inherit cargoArtifacts;
      __noChroot = true;
      pname = "xmtpv3-${target}";
      src = bindingsFileset;
      inherit (rust.crateNameFromCargoToml {
        cargoToml = ./../../Cargo.toml;
      }) version;
      cargoExtraArgs = "--target ${target} -p xmtpv3";
      preBuild = iosEnv.envSetup target;
      installPhaseCommand = ''
        mkdir -p $out/${target}
        cp target/${target}/release/libxmtpv3.a $out/${target}/
      '';
    });

  # Per-target derivations
  targets = lib.genAttrs iosEnv.iosTargets buildTarget;

  # Native host env setup for Swift bindings (builds for macOS host, not iOS)
  nativeEnvSetup = iosEnv.envSetup "aarch64-apple-darwin";

  # Swift bindings derivation (impure — needs Xcode SDK for native host build)
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
    doNotPostBuildInstallCargoBinaries = true;
    installPhaseCommand = ''
      ${nativeEnvSetup}
      # Generate Swift bindings using uniffi-bindgen
      cargo run -p xmtpv3 --bin ffi-uniffi-bindgen --release --features uniffi/cli generate \
        --library target/release/libxmtpv3.a \
        --out-dir $TMPDIR/swift-out \
        --language swift

      # Organize into expected directory structure
      mkdir -p $out/swift/include/libxmtp
      cp $TMPDIR/swift-out/xmtpv3.swift $out/swift/
      mv $TMPDIR/swift-out/xmtpv3FFI.h $out/swift/include/libxmtp/
      mv $TMPDIR/swift-out/xmtpv3FFI.modulemap $out/swift/include/libxmtp/module.modulemap
    '';
  });

  # Aggregate: combines all targets + swift bindings
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
