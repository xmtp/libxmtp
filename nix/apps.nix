_: {
  perSystem =
    { pkgs, ... }:
    {
      packages = {
        xdbg = pkgs.callPackage ./package/xdbg.nix { };
        xnet-cli = pkgs.callPackage ./package/xnet-cli.nix { };
      };
    };
}
