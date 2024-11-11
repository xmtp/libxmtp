# Function that takes buildTargets and builds the `crate` package against it
{ mkPkgs
, eachCrossSystem
, mkToolchain
, crane
, system
}:
let
  buildTargets = import ./buildtargets.nix;
in eachCrossSystem {
  buildSystem = system;
  supportedSystems = builtins.attrNames buildTargets;
  mkDerivationFor = buildSystem: hostSystem: let
    pkgs = mkPkgs buildSystem null buildTargets;
    pkgsCross = mkPkgs buildSystem hostSystem buildTargets;
    inherit (buildTargets.${hostSystem}) rustTarget;
    rust-toolchain = mkToolchain rustTarget;
    craneLib = (crane.mkLib pkgs).overrideToolchain (p: rust-toolchain);
  in
    pkgs.callPackage ./crate.nix { inherit craneLib rustTarget pkgsCross; };
}
