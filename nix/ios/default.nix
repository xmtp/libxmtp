# Function that takes buildTargets and builds the `crate` package against it
{ mkPkgs
, eachCrossSystem
, mkToolchain
, crane
}:
let
  buildTargets = import ./buildtargets.nix;
in
eachCrossSystem {
  supportedSystems = builtins.attrNames buildTargets;
  mkDerivationFor = hostSystem:
    let
      pkgs = mkPkgs hostSystem buildTargets;
      # pkgsCross = mkPkgs hostSystem buildTargets;
      inherit (buildTargets.${hostSystem}) rustTarget;
      rust-toolchain = mkToolchain rustTarget;
      craneLib = (crane.mkLib pkgs).overrideToolchain (p: rust-toolchain);
    in
    pkgs.callPackage ./crate.nix { inherit craneLib rustTarget; };
}
