{
  xmtp,
  stdenv,
  ...
}:
let
  inherit (xmtp) base craneLib;
  rust-toolchain = p: xmtp.mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ ];
  rust = craneLib.overrideToolchain rust-toolchain;
  version = xmtp.mkVersion rust;
  inherit (base) commonArgs bindingsFileset;

  cargoArtifacts = xmtp.base.mkCargoArtifacts rust false null;

  dylib = rust.buildPackage (
    commonArgs
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
