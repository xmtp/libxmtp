# The generated SDK native library. Keep the old binding derivations separate.
{
  lib,
  xmtp,
  stdenv,
  stdenvNoCC,
  python3,
  cargo-zigbuild,
  glibcVersion ? null,
  android ? false,
  crateName ? "xmtp_sdk",
}:
let
  target = stdenv.hostPlatform.rust.rustcTarget;
  rust = xmtp.craneLib.overrideToolchain (p: xmtp.mkToolchain p [ target ] [ ]);
  sources = import ../lib/sdk-sources.nix { inherit lib xmtp; };
  source = if crateName == "xmtp_sdk" then sources.sdk rust else sources.legacyMigration rust;
  mkProvenance = import ../lib/sdk-provenance.nix { inherit stdenvNoCC python3; };
  isGnu = stdenv.hostPlatform.isLinux && !stdenv.hostPlatform.isMusl && glibcVersion != null;
  buildTarget = target + lib.optionalString isGnu ".${glibcVersion}";
  special = {
    OPENSSL_NO_VENDOR = "0";
    OPENSSL_STATIC = "1";
  }
  // lib.optionalAttrs stdenv.hostPlatform.isDarwin {
    MACOSX_DEPLOYMENT_TARGET = "11.0";
    # Darwin setup replaces this variable before the Cargo build.
    preBuild = "export MACOSX_DEPLOYMENT_TARGET=11.0";
  }
  // lib.optionalAttrs android { buildInputs = [ ]; }
  // lib.optionalAttrs stdenv.hostPlatform.isMusl { RUSTFLAGS = "-C target-feature=-crt-static"; };
  command =
    lib.optionalString isGnu "CARGO_ZIGBUILD_CACHE_DIR=$TMPDIR/cargo-zigbuild ZIG_GLOBAL_CACHE_DIR=$TMPDIR/zig-global "
    + "cargo ${
      if isGnu then "zigbuild" else "build"
    } --release --locked -p ${crateName} --lib --target ${buildTarget}";
  compilation = rust.buildPackage (
    xmtp.base.commonArgs
    // special
    // {
      pname = "${lib.replaceStrings [ "_" ] [ "-" ] crateName}-native-${target}";
      nativeBuildInputs =
        xmtp.base.commonArgs.nativeBuildInputs ++ lib.optionals isGnu [ cargo-zigbuild ];
      version = xmtp.mkVersion rust;
      src = source;
      cargoArtifacts = xmtp.base.mkCargoArtifacts rust false (
        special
        // {
          CARGO_BUILD_TARGET = buildTarget;
          nativeBuildInputs =
            xmtp.base.commonArgs.nativeBuildInputs ++ lib.optionals isGnu [ cargo-zigbuild ];
          buildPhaseCargoCommand = command;
        }
      );
      CARGO_BUILD_TARGET = buildTarget;
      buildPhaseCargoCommand = command;
      doNotPostBuildInstallCargoBinaries = true;
      installPhaseCommand = ''
        mkdir -p $out/lib
        cp target/${target}/release/lib${crateName}.a $out/lib/
        cp target/${target}/release/lib${crateName}.${
          if stdenv.hostPlatform.isDarwin then "dylib" else "so"
        } $out/lib/
      '';
    }
  );

in
mkProvenance {
  inherit compilation target;
  source = sources.provenanceSource;
}
