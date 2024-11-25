{ pkgs, craneLib, filesets }:
let
  mls_validation_service = pkgs.callPackage ./crate.nix { inherit craneLib filesets; };
  muslService = pkgs.pkgsCross.musl64.callPackage ./crate.nix {
    inherit craneLib filesets; rustTarget = "x86_64-unknown-linux-musl";
  };

  dockerImage = pkgs.dockerTools.buildImage {
    name = "mls_validation_service";
    tag = "latest";
    copyToRoot = [ muslService.bin ];
    config = {
      Cmd = [ "${muslService.bin}/bin/xdbg --help" ];
    };
  };
in
{

  inherit (mls_validation_service) bin devShell;
  inherit muslService;
  inherit dockerImage;
}

