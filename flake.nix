# Flake Shell for building release artifacts for swift and kotlin
# Learn about nix: https://nix.dev
# Consistent with `nix` terminology, the `build` system is the machine _building_ the package,
#   while the `host` system is where the package _will_ run.
{
  description = "Flake for building & cross-compiling the components of libxmtp in one deterministic place";

  inputs = {
    # The nix package set (stable)
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";

    # An nix-native and customizable overlay for Rust
    fenix = {
      url = "github:nix-community/fenix";
      inputs = { nixpkgs.follows = "nixpkgs"; };
    };
    # A nix-native packaging system for rust (docs: https://crane.dev/index.html)
    crane.url = "github:ipetkov/crane";
    flake-utils = { url = "github:numtide/flake-utils"; };
  };

  outputs = { nixpkgs, flake-utils, fenix, crane, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          # Rust Overlay
          overlays = [ fenix.overlays.default ];
          config = {
            android_sdk.accept_license = true;
            allowUnfree = true;
          };
        };
        inherit (import ./nix/util.nix system) eachCrossSystem;

        # Function to make a toolchain that includes a foreign target
        mkToolchain = with fenix.packages.${system}; target: combine [
          stable.cargo
          stable.rustc
          targets.${target}.stable.rust-std
        ];

        # Function to create a package set with an optional foreign host
        mkPkgs = hostSystem: buildTargets: import nixpkgs ({
          localSystem = system;
        } // (if hostSystem == system then { } else {
          # The nixpkgs cache doesn't have any packages where cross-compiling has
          # been enabled, even if the host platform is actually the same as the
          # build platform (and therefore it's not really cross-compiling). So we
          # only set up the cross-compiling config if the host platform is
          # different.
          crossSystem.config = buildTargets.${hostSystem}.crossSystemConfig;
          # crossSystem = buildTargets.${hostSystem};
        }));

        iosPackages = import ./nix/ios { inherit mkPkgs eachCrossSystem mkToolchain crane; };
      in
      {
        # The shell where android and iOS can be built from
        devShells.default = pkgs.callPackage ./nix/buildshell.nix { };
        packages = {
          ios = iosPackages;
          # ios = pkgs.callPackage ./nix { inherit craneLib; };
          # android
          # wasm
          # validation service
        };
      });
}
