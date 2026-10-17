_: {
  perSystem =
    { pkgs, ... }:
    {
      packages = {
        xdbg = pkgs.callPackage ./package/xdbg.nix { };
      };
    };
}
