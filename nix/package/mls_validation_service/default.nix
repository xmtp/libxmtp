{ pkgs
, craneLib
, filesets
, mkToolchain
,
}:
let
  mls_validation_service = pkgs.callPackage ./crate.nix { inherit filesets; craneLib = craneLib.overrideToolchain (mkToolchain [ ] [ ]); };
  muslService = pkgs.pkgsCross.musl64.callPackage ./crate.nix {
    inherit filesets; craneLib = craneLib.overrideToolchain (mkToolchain [ "x86_64-unknown-linux-musl" ] [ ]);
    rustTarget = "x86_64-unknown-linux-musl";
  };

  dockerImage = pkgs.dockerTools.buildLayeredImage {
    name = "ghcr.io/xmtp/mls-validation-service"; # override ghcr images
    tag = "main";
    architecture = "amd64";
    contents = [ muslService.bin ];
    config = {
      Cmd = [ "${muslService.bin}/bin/mls-validation-service" ];
    };
  };
in
{

  inherit (mls_validation_service) bin devShell;
  inherit dockerImage;
}
