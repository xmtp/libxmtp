# Shared configuration for mobile (iOS/Android) cross-compilation derivations.
# Centralizes common build arguments, filesets, and version extraction.
{
  lib,
  xmtp,
  zstd,
  openssl,
  sqlite,
  pkg-config,
  darwin,
  stdenv,
  zlib,
  pkgsBuildHost,
  perl,
}:
let
  # The protobuf schemas and the one real build script that a dependency
  # layer keeps. Other files reach a layer only through the cleaned
  # manifests, so a comment or another build script leaves it unchanged.
  protoSource = lib.fileset.toSource {
    root = ./../..;
    fileset = lib.fileset.unions [
      ./../../proto
      ./../../crates/xmtp_proto/build.rs
    ];
  };

  # Crane reads target paths from the full workspace, then launders source
  # contents from its dependency-only dummy source.
  dummyInputFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = xmtp.filesets.workspace;
  };

  # The suffix strictly for the build platform/native machine building the code
  # _not_ the compilation target (which is different when cross-compiling)
  buildPlatformSuffix =
    builtins.replaceStrings [ "-" ] [ "_" ]
      pkgsBuildHost.stdenv.hostPlatform.rust.rustcTarget;

  # Common build arguments shared between iOS and Android derivations.
  # Platform-specific args (like ANDROID_HOME or __noChroot) are added by each derivation.
  commonArgs = {
    # strictDeps=true breaks darwin build with ring
    strictDeps = if stdenv.buildPlatform.isDarwin then false else true;
    # these inputs do not get cross compiled
    nativeBuildInputs = [
      pkg-config
      perl
      zlib
      pkgsBuildHost.protobuf
    ]
    ++ lib.optionals stdenv.buildPlatform.isDarwin [ darwin.libiconv ]
    # crane#1059 stopped adding toolchain cc's to nativeBuildInputs; restore the
    # build-platform cc so its setup hook feeds build-role NIX_LDFLAGS (-liconv
    # for darwin build scripts) and keeps unprefixed `cc` on PATH (aws-lc-sys).
    ++ lib.optionals (stdenv.buildPlatform != stdenv.hostPlatform) [ pkgsBuildHost.stdenv.cc ];
    # these inputs do get cross compiled
    buildInputs = [
      zstd
      openssl
      sqlite
    ]
    ++ lib.optionals stdenv.hostPlatform.isDarwin [ darwin.libiconv ];

    doCheck = false;
    # Disable zerocallusedregs hardening which can cause issues with cross-compilation.
    hardeningDisable = [ "zerocallusedregs" ];
    CARGO_BUILD_TARGET = stdenv.hostPlatform.rust.rustcTarget;
    CARGO_PROFILE = "release";
    doInstallCargoArtifacts = false;

    # aws-lc-sys is tricky to x-compile, since it needs host CC to compile libraries to do host-side checks.
    # aws-lc-sys's build script resolves CC via TARGET_CC (set by Nix to the cross-compiler) and
    # overwrites CC_<host_target>. Setting AWS_LC_SYS_TARGET_CC_<host> intercepts this resolution
    # so host-side compiler checks use the native cc instead of the Android cross-compiler.
    # https://crane.dev/faq/cross-compiling-aws-lc-sys.html
    "AWS_LC_SYS_TARGET_CC_${buildPlatformSuffix}" = "cc";
    "AWS_LC_SYS_TARGET_CXX_${buildPlatformSuffix}" = "c++";

  };

  # Crane reads the lock file and the version from `src` during evaluation.
  # A dummy source is a derivation, so derivations that build from one pass
  # these values directly. Evaluation then needs no import from derivation and
  # works for another system. Git checkouts use the toolchain's Cargo, so the
  # vendor directory comes from the caller's Crane library.
  mkCargoVendorDir = rust: rust.vendorCargoDeps { cargoLock = ./../../Cargo.lock; };

  # Make the dependency layer of a Crane derivation. `rust` is the caller's
  # Crane library. `overrides` sets `buildPhaseCargoCommand` to the consumer's
  # own command, with the same packages, targets, profile, and environment, so
  # the final build compiles no third-party crate again.
  mkCargoArtifacts =
    rust: overrides:
    assert lib.assertMsg (
      overrides ? buildPhaseCargoCommand
    ) "mkCargoArtifacts: set buildPhaseCargoCommand to the consumer's Cargo command and -p list";
    let
      dummySrc = rust.mkDummySrc {
        src = dummyInputFileset;
        extraDummyScript = ''
          mkdir -p $out/proto $out/crates/xmtp_proto
          cp --recursive ${protoSource}/proto/. $out/proto
          cp ${protoSource}/crates/xmtp_proto/build.rs $out/crates/xmtp_proto/build.rs
          xmtp_dummy_build=$(grep '^build = ' $out/crates/xmtp_proto/Cargo.toml)
          substituteInPlace $out/crates/xmtp_proto/Cargo.toml \
            --replace-fail "$xmtp_dummy_build" 'build = "build.rs"'
        '';
      };
    in
    rust.buildDepsOnly (
      commonArgs
      // {
        inherit dummySrc;
        cargoVendorDir = mkCargoVendorDir rust;
        pname = "cargo-package";
        version = xmtp.mkVersion rust;
        doInstallCargoArtifacts = true;
      }
      // overrides
    );

in
{
  inherit
    commonArgs
    mkCargoVendorDir
    mkCargoArtifacts
    ;
}
