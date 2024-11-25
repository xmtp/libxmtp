{
  pkgs,
  craneLib,
  filesets,
}:
let
  xdbg = pkgs.callPackage ./crate.nix { inherit craneLib filesets; };
  muslXDbg = pkgs.pkgsCross.musl64.callPackage ./crate.nix {
    inherit craneLib filesets;
    rustTarget = "x86_64-unknown-linux-musl";
  };

  dockerImage = pkgs.dockerTools.streamLayeredImage {
    name = "xmtp/xdbg";
    tag = "latest";
    contents = [ muslXDbg.bin ];
    config = {
      Cmd = [ "${muslXDbg.bin}/bin/xdbg" ];
    };
  };
in
{

  inherit (xdbg) bin devShell;
  inherit muslXDbg;
  inherit dockerImage;
}
