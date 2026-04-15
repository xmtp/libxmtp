{
  xmtp,
  stdenv,
  ...
}:
let
  inherit (xmtp) craneLib base;
  rust-toolchain = p: xmtp.mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ ];
  rust = craneLib.overrideToolchain rust-toolchain;
  version = xmtp.mkVersion rust;
  inherit (base) bindingsFileset commonArgs;

  cargoArtifacts = xmtp.base.mkCargoArtifacts rust false { };

  ext = if stdenv.isDarwin then "dylib" else "so";
  dylib = rust.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts version;
      pname = "xmtpv3-jvm-${stdenv.hostPlatform.rust.rustcTarget}";
      doInstallCargoArtifacts = false;
      src = bindingsFileset;
      cargoExtraArgs = "-p xmtpv3";
      postFixup = ''
        cp $out/lib/libxmtpv3.${ext} $out/libuniffi_xmtpv3.${ext}
        rm -rf $out/lib
      '';
    }
  );
in
{
  inherit dylib;
}
