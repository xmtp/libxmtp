# Shared configuration for mobile (iOS/Android) cross-compilation derivations.
# Centralizes common build arguments, filesets, and version extraction.
{
  lib,
  xmtp,
  zstd,
  openssl,
  sqlite,
  pkg-config,
  perl,
  darwin,
  stdenv,
  zlib,
  pkgsBuildHost,
}:
let
  # Narrow fileset for buildDepsOnly — only Cargo.toml, Cargo.lock, build.rs,
  # and files referenced by build scripts. Source (.rs) changes don't invalidate
  # the dep cache since crane replaces them with dummies anyway.
  depsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = xmtp.filesets.depsOnly;
  };

  # Full fileset for buildPackage — includes all source files needed to compile
  # the xmtpv3 crate and its workspace dependencies.
  bindingsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = xmtp.filesets.forCrate ./../../bindings/mobile;
  };

  # The suffix strictly for the build platform/native machine building the code
  # _not_ the compilation target (which is different when cross-compiling)
  buildPlatformSuffix =
    builtins.replaceStrings [ "-" ] [ "_" ]
      pkgsBuildHost.stdenv.hostPlatform.rust.rustcTarget;

  # Common build arguments shared between iOS and Android derivations.
  # Platform-specific args (like ANDROID_HOME or __noChroot) are added by each derivation.
  commonArgs = {
    src = depsFileset;
    # strictDeps=true breaks darwin build with ring
    strictDeps = if stdenv.buildPlatform.isDarwin then false else true;
    # these inputs do not get cross compiled
    nativeBuildInputs = [
      pkg-config
      perl
      zlib
    ]
    ++ lib.optionals stdenv.buildPlatform.isDarwin [ darwin.libiconv ];
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

    # aws-lc-sys is tricky to x-compile, since it needs host CC to compile libraries to do host-side checks.
    # aws-lc-sys's build script resolves CC via TARGET_CC (set by Nix to the cross-compiler) and
    # overwrites CC_<host_target>. Setting AWS_LC_SYS_TARGET_CC_<host> intercepts this resolution
    # so host-side compiler checks use the native cc instead of the Android cross-compiler.
    # https://crane.dev/faq/cross-compiling-aws-lc-sys.html
    "AWS_LC_SYS_TARGET_CC_${buildPlatformSuffix}" = "cc";
    "AWS_LC_SYS_TARGET_CXX_${buildPlatformSuffix}" = "c++";

  };

  # Make cargo artifacts for a derivation building rust code
  # "rust" is the rust toolchain to use (native or host)
  # "test" is whether to use "test-utils" feature
  # "zigbuild" uses "cargo zigbuild" instead of "cargo build"
  mkCargoArtifacts =
    rust: test: overrides:
    let
      maybeTestFeature = if test then "--features test-utils" else "";
      overrides' = if overrides == null then { } else overrides;
    in
    rust.buildDepsOnly (
      commonArgs
      // {
        src = rust.cleanCargoSource ../..;
        buildPhaseCargoCommand = "cargo build ${maybeTestFeature} --profile $CARGO_PROFILE --locked";
      }
      // overrides'
    );

in
{
  inherit
    depsFileset
    bindingsFileset
    commonArgs
    mkCargoArtifacts
    ;
}
