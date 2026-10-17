# The SDK native library of one package set. The host set gives the one host
# library: renders, the host Node package, and the JNA test host load it.
# glibc targets build with zigbuild against the glibc floor.
{
  lib,
  xmtp,
  stdenv,
  stdenvNoCC,
  buildPackages,
  cargo-zigbuild,
  android ? false,
}:
let
  target = stdenv.hostPlatform.rust.rustcTarget;
  rust = xmtp.craneLib.overrideToolchain xmtp.hostToolchain;
  sources = import ../lib/sdk-sources.nix { inherit lib xmtp; };
  source = sources.sdk rust;
  mkProvenance = import ../lib/sdk-provenance.nix { inherit stdenvNoCC; };
  isGnu = stdenv.hostPlatform.isLinux && stdenv.hostPlatform.isGnu;
  buildTarget = target + lib.optionalString isGnu ".${xmtp.sdkPlatforms.glibcFloor}";
  special = {
    OPENSSL_NO_VENDOR = "0";
    OPENSSL_STATIC = "1";
  }
  // lib.optionalAttrs stdenv.hostPlatform.isDarwin {
    MACOSX_DEPLOYMENT_TARGET = "11.0";
    # Darwin setup replaces this variable before the Cargo build.
    preBuild = "export MACOSX_DEPLOYMENT_TARGET=11.0";
  }
  # rustc's release default `strip = "debuginfo"` passes `--strip-debug`, and
  # zig's linker strips the whole static symbol table for it. UniFFI reads its
  # metadata from that table, so the renders of this host library need it.
  # The fixup phase still removes debug sections.
  // lib.optionalAttrs isGnu { CARGO_PROFILE_RELEASE_STRIP = "none"; }
  // lib.optionalAttrs android { buildInputs = [ ]; }
  // lib.optionalAttrs stdenv.hostPlatform.isMusl { RUSTFLAGS = "-C target-feature=-crt-static"; };
  command =
    lib.optionalString stdenv.hostPlatform.isDarwin (
      xmtp.iosEnv.nativeBuildEnv {
        cc = "${stdenv.cc}/bin/cc";
        cxx = "${stdenv.cc}/bin/c++";
      }
    )
    + lib.optionalString isGnu "CARGO_ZIGBUILD_CACHE_DIR=$TMPDIR/cargo-zigbuild ZIG_GLOBAL_CACHE_DIR=$TMPDIR/zig-global "
    + "cargo ${
      if isGnu then "zigbuild" else "build"
    } --release --locked${lib.optionalString stdenv.hostPlatform.isDarwin " ${xmtp.iosEnv.nativeCargoArgs}"} -p xmtp_sdk --lib --target ${buildTarget}";
  compilation = rust.buildPackage (
    xmtp.base.commonArgs
    // special
    // {
      pname = "xmtp-sdk-native-${target}";
      nativeBuildInputs =
        xmtp.base.commonArgs.nativeBuildInputs
        ++ lib.optionals isGnu [ cargo-zigbuild ]
        ++ lib.optionals android [ buildPackages.removeReferencesTo ];
      version = xmtp.mkVersion rust;
      src = source;
      cargoVendorDir = xmtp.base.mkCargoVendorDir rust;
      cargoArtifacts = xmtp.base.mkCargoArtifacts rust (
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
        cp target/${target}/release/libxmtp_sdk.a $out/lib/
        cp target/${target}/release/libxmtp_sdk.${
          if stdenv.hostPlatform.isDarwin then "dylib" else "so"
        } $out/lib/
      ''
      # OpenSSL embeds its compiler command, which names the NDK wrapper.
      # Only OpenSSL_version(OPENSSL_CFLAGS) reads it, so the libraries
      # need no NDK at run time. strip keeps it: the text is in .rodata.
      + lib.optionalString android ''
        remove-references-to -t ${stdenv.cc} $out/lib/libxmtp_sdk.so $out/lib/libxmtp_sdk.a
      '';
    }
    # Reject the NDK wrapper and the paths it wraps: the unwrapped toolchain,
    # the bintools wrapper, and bionic. All are in the wrapper's closure, which
    # this build already needs, so the guard adds no fetch.
    // lib.optionalAttrs android {
      disallowedRequisites = [
        stdenv.cc
        stdenv.cc.cc
        stdenv.cc.bintools
        stdenv.cc.libc
      ];
    }
  );

in
mkProvenance {
  compilation = xmtp.base.assertNativeEnv {
    drv = compilation;
    macos = stdenv.hostPlatform.isDarwin;
  };
  inherit target;
  inherit (sources) provenance;
}
