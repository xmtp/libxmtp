{
  xmtp,
  lib,
}:
let
  inherit (xmtp) base;
  rust-toolchain = p: xmtp.mkToolchain p [ ] [ ];
  rust = xmtp.craneLib.overrideToolchain rust-toolchain;
  cargoArtifacts = xmtp.base.mkCargoArtifacts rust false null;
  src = ./../../..;
in
rust.buildPackage (
  base.commonArgs
  // {
    inherit cargoArtifacts;
    pname = "ffi-uniffi-bindgen";
    cargoExtraArgs = "-p xmtpv3 --bin ffi-uniffi-bindgen --features uniffi/cli";
    version = xmtp.mkVersion rust;
    src = lib.fileset.toSource {
      root = src;
      fileset = xmtp.filesets.forCrate (src + /bindings/mobile);
    };
  }
)
