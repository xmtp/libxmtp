{
  lib,
  xmtp,
  cacert,
  stdenv,
}:
let
  inherit (xmtp) base craneLib mkToolchain;
  inherit (craneLib.fileset) commonCargoSources;
  rust-toolchain = p: mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ ];
  rust = xmtp.craneLib.overrideToolchain rust-toolchain;

  commonArgs = base.commonArgs // {
    nativeBuildInputs = base.commonArgs.nativeBuildInputs ++ [ cacert ];
  };
  root = ./../..;
  cargoArtifacts = base.mkCargoArtifacts rust false null;
in
rust.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    src = lib.fileset.toSource {
      inherit root;
      fileset = xmtp.filesets.forCrate (commonCargoSources (root + /apps/xmtp_debug));
    };
    NIX_GIT_SHA = xmtp.gitSha;
    NIX_GIT_COMMIT_DATE = xmtp.gitCommitDate;
    version = xmtp.mkVersion rust;
    pname = "xdbg";
    cargoExtraArgs = "-p xdbg";
  }
)
