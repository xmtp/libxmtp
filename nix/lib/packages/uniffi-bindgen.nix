{ xmtp
, lib
}:
let
  inherit (xmtp) mobile;
  rust-toolchain = xmtp.mkToolchain [ ] [ ];
  rust = xmtp.craneLib.overrideToolchain (p: rust-toolchain);
  cargoArtifacts = rust.buildDepsOnly mobile.commonArgs;
  src = ./../../..;
in
rust.buildPackage
  (mobile.commonArgs // {
    inherit cargoArtifacts;
    pname = "ffi-uniffi-bindgen";
    cargoExtraArgs = "-p xmtpv3 --bin ffi-uniffi-bindgen --features uniffi/cli";
    version = xmtp.mobile.mkVersion rust;
    src = lib.fileset.toSource {
      root = src;
      fileset = xmtp.filesets.forCrate (src + /bindings/mobile);
    };
  })
