{ pkgs
, craneLib
, filesets
, mkToolchain
,
}:
let
  mls_validation_service = pkgs.callPackage ./crate.nix { inherit filesets; craneLib = craneLib.overrideToolchain (mkToolchain [ ] [ ]); };

  musl = pkgs.pkgsCross.musl64.callPackage ./crate.nix {
    inherit filesets; craneLib = craneLib.overrideToolchain (mkToolchain [ "x86_64-unknown-linux-musl" ] [ ]);
    rustTarget = "x86_64-unknown-linux-musl";
  };

  dockerImage = pkgs.pkgsCross.musl64.dockerTools.buildLayeredImage {
    name = "ghcr.io/xmtp/mls-validation-service"; # override ghcr images
    tag = "main";
    architecture = "amd64";
    created = "now";
    contents = [ musl.bin ];
    config = {
      Env = [
        "ANVIL_URL=http://localhost:8545"
      ];
      Cmd = [ "${musl.bin}/bin/mls-validation-service" ];
    };
  };
in
{

  inherit (mls_validation_service) bin devShell;
  inherit dockerImage;
  inherit musl;
}
