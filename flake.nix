# Flake Shell for building release artifacts for swift and kotlin
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    fenix = {
      url = "github:nix-community/fenix";
      inputs = { nixpkgs.follows = "nixpkgs"; };
    };
    flake-parts = { url = "github:hercules-ci/flake-parts"; };
    systems.url = "github:nix-systems/default";
    mkshell-util.url = "github:insipx/mkShell-util.nix";
  };

  nixConfig = {
    extra-trusted-public-keys = "xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0=";
    extra-substituters = "https://xmtp.cachix.org";
  };

  outputs = inputs@{ flake-parts, fenix, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;
      perSystem = { pkgs, system, ... }:
        let
          util = import inputs.mkshell-util;
          mkShellWrappers = pkgs: util callPackage pkgs;
          callPackage = pkgs: pkgs.lib.callPackageWith ((mkShellWrappers pkgs) // pkgs);
          pkgConfig = {
            inherit system;
            # Rust Overlay
            overlays = [ fenix.overlays.default ];
            config = {
              android_sdk.accept_license = true;
              allowUnfree = true;
            };
          };
        in
        {
          _module.args.pkgs = import inputs.nixpkgs pkgConfig;
          devShells = {
            # shell for general xmtp rust dev
            default = callPackage pkgs ./nix/libxmtp.nix { };
            # Shell for android builds
            android = callPackage pkgs ./nix/android.nix { };
            # Shell for iOS builds
            ios = callPackage pkgs ./nix/ios.nix { };
          };
        };
    };
}
