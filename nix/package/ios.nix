{ lib
, fenix
, zstd
, openssl
, sqlite
, pkg-config
, craneLib
, xmtp
, stdenv
, ...
}:
let
  iosTargets = [
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
    "aarch64-apple-ios"
    "aarch64-apple-ios-sim"
  ];

  # Pinned Rust Version with iOS targets
  rust-toolchain = xmtp.mkToolchain iosTargets [ "clippy-preview" "rustfmt-preview" ];
  rust = craneLib.overrideToolchain (p: rust-toolchain);

  libraryFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = (xmtp.filesets { inherit lib craneLib; }).libraries;
  };

  bindingsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = (xmtp.filesets { inherit lib craneLib; }).forCrate ./../../bindings/mobile;
  };

  # Common Xcode/iOS environment variables (mirrors nix/ios.nix shellHook)
  developerDir = "/Applications/Xcode.app/Contents/Developer";
  iosSdk = "${developerDir}/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk";
  iosSimSdk = "${developerDir}/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk";

  iosEnv = {
    DEVELOPER_DIR = developerDir;
    CC_aarch64_apple_ios = "/usr/bin/clang";
    CXX_aarch64_apple_ios = "/usr/bin/clang++";
    CC_aarch64_apple_ios_sim = "/usr/bin/clang";
    CXX_aarch64_apple_ios_sim = "/usr/bin/clang++";
    CARGO_TARGET_AARCH64_APPLE_IOS_LINKER = "/usr/bin/clang";
    CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER = "/usr/bin/clang";
    BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios = "--target=arm64-apple-ios --sysroot=${iosSdk}";
    BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios_sim = "--target=arm64-apple-ios-simulator --sysroot=${iosSimSdk}";
  };

  commonArgs = {
    src = libraryFileset;
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ zstd openssl sqlite ];
    doCheck = false;
    # For iOS cross-compilation, openssl must be vendored (builds from source per target)
    # Do NOT set OPENSSL_NO_VENDOR
    hardeningDisable = [ "zerocallusedregs" ];
  };

  # Build a static library for a single target
  buildTarget = target:
    let
      targetEnv = iosEnv // {
        CARGO_BUILD_TARGET = target;
      };

      # Dep caching: only rebuilds when Cargo.lock changes
      cargoArtifacts = rust.buildDepsOnly (targetEnv // commonArgs // {
        pname = "xmtpv3-deps-${target}";
        # Impure: needs Xcode SDK for bindgen during dep compilation
        __noChroot = true;
        cargoExtraArgs = "--target ${target} -p xmtpv3";
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
      installPhaseCommand = ''
        mkdir -p $out/${target}
        cp target/${target}/release/libxmtpv3.a $out/${target}/
      '';
    });

  # Per-target derivations
  targets = lib.genAttrs iosTargets buildTarget;

  # Swift bindings derivation (pure — native host build only)
  swiftBindings = rust.buildPackage (commonArgs // {
    pname = "xmtpv3-swift-bindings";
    src = bindingsFileset;
    inherit (rust.crateNameFromCargoToml {
      cargoToml = ./../../Cargo.toml;
    }) version;
    cargoArtifacts = rust.buildDepsOnly (commonArgs // {
      pname = "xmtpv3-swift-bindings-deps";
      cargoExtraArgs = "-p xmtpv3";
    });
    cargoExtraArgs = "-p xmtpv3";
    buildPhaseCargoCommand = ''
      cargo build --release -p xmtpv3
    '';
    installPhaseCommand = ''
      # Generate Swift bindings using uniffi-bindgen
      cargo run --bin ffi-uniffi-bindgen --release --features uniffi/cli generate \
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
      '') iosTargets}
      ln -s ${swiftBindings}/swift/xmtpv3.swift $out/swift/xmtpv3.swift
      ln -s ${swiftBindings}/swift/include $out/swift/include
    '';
  };

in
{
  inherit targets swiftBindings aggregate;
}
