# CI checks to ensure all packages and dev shells build
_: {
  perSystem =
    { pkgs, ... }:
    {
      checks.nextest-v3 = pkgs.callPackage ./package/nextest.nix { };
      checks.nextest-d14n = pkgs.callPackage ./package/nextest.nix { d14n = true; };
    };
}
