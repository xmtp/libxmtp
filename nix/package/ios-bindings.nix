{
  lib,
  xmtp,
  stdenv,
  openssl,
  # Shipped artifacts (xcframework inputs) link OpenSSL statically —
  # consumers' devices have no /nix/store. The native bindings-generation
  # build never ships, so it keeps the cheaper dynamic default.
  staticOpenssl ? true,
  ...
}:
let
  inherit (xmtp) base craneLib;
  rust-toolchain = p: xmtp.mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ ];
  rust = craneLib.overrideToolchain rust-toolchain;
  version = xmtp.mkVersion rust;
  inherit (base) commonArgs bindingsFileset;

  opensslStatic = openssl.override { static = true; };
  opensslArgs = lib.optionalAttrs staticOpenssl (
    base.opensslEnv opensslStatic
    // {
      buildInputs = lib.remove openssl commonArgs.buildInputs ++ [ opensslStatic ];
      OPENSSL_STATIC = "1";
    }
  );

  cargoArtifacts = xmtp.base.mkCargoArtifacts rust false opensslArgs;

  targetPrefix = stdenv.cc.targetPrefix or "";

  dylib = rust.buildPackage (
    commonArgs
    // opensslArgs
    // {
      inherit cargoArtifacts version;
      pname = "xmtpv3";
      doInstallCargoArtifacts = false;
      src = bindingsFileset;
      cargoExtraArgs = "-p xmtpv3";
      postFixup = ''
        mkdir -p $out
        cp $out/lib/libxmtpv3.a $out/libxmtpv3.a
        cp $out/lib/libxmtpv3.dylib $out/libxmtpv3.dylib
        install_name_tool -id @rpath/libxmtpv3.dylib $out/libxmtpv3.dylib
      ''
      + lib.optionalString staticOpenssl ''
        # Shipped artifacts must be self-contained: a /nix/store load
        # command is a dyld crash on device, and undefined OpenSSL symbols
        # in the archive push an unresolvable link burden onto consumers.
        if ${targetPrefix}otool -L $out/libxmtpv3.dylib | grep '/nix/store/'; then
          echo "FAIL: libxmtpv3.dylib links nix store libraries"; exit 1
        fi
        if ${targetPrefix}nm -u $out/libxmtpv3.a 2>/dev/null | grep -qE '_SSL_|_CRYPTO_malloc|_EVP_'; then
          echo "FAIL: libxmtpv3.a has undefined OpenSSL symbols"; exit 1
        fi
      '';
    }
  );

  swift-bindings = rust.uniffiGenerate {
    inherit version;
    pname = "xmtpv3-swift";
    language = "swift";
    dylibPath = "${dylib}/libxmtpv3.a";
    doInstallCargoArtifacts = false;
    postFixup = ''
      mkdir -p $out/swift/include/libxmtp
      ls $out/swift
      # Organize into expected directory structure for xcframework assembly
      mv $out/swift/xmtpv3FFI.h $out/swift/include/libxmtp/
      mv $out/swift/xmtpv3FFI.modulemap $out/swift/include/libxmtp/module.modulemap

      # Generate version file. Commit date, not wall clock — keeps the
      # output reproducible across days.
      echo "Version: ${version}" > $out/libxmtp-version.txt
      echo "Date: ${xmtp.gitCommitDate}" >> $out/libxmtp-version.txt
    '';
  };
in
{
  inherit swift-bindings dylib version;
}
