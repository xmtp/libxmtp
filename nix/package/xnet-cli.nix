{
  lib,
  xmtp,
  stdenv,
}:
let
  inherit (xmtp) craneLib base mkToolchain;
  inherit (xmtp.base) commonArgs;
  inherit (craneLib.fileset) commonCargoSources;
  rust-toolchain = p: mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ ];
  rust = xmtp.craneLib.overrideToolchain rust-toolchain;

  root = ./../..;
  cargoArtifacts = base.mkCargoArtifacts rust false null;
in
rust.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    src = lib.fileset.toSource {
      inherit root;
      fileset = xmtp.filesets.forCrate [
        (commonCargoSources (root + /apps/xnet/lib))
        (commonCargoSources (root + /apps/xnet/cli))
      ];
    };
    version = xmtp.mkVersion rust;
    NIX_GIT_SHA = xmtp.gitSha;
    NIX_GIT_COMMIT_DATE = xmtp.gitCommitDate;
    pname = "xnet-cli";
    cargoExtraArgs = "-p xnet-cli";
  }
)
