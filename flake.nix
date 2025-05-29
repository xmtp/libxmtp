{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs = { nixpkgs.follows = "nixpkgs"; };
    };
    flake-parts = { url = "github:hercules-ci/flake-parts"; };
    systems.url = "github:nix-systems/default";
    foundry.url = "github:shazow/foundry.nix/monthly";
    crane = {
      url = "github:ipetkov/crane";
    };
    rust-manifest = {
      url = "https://static.rust-lang.org/dist/channel-rust-stable.toml";
      flake = false;
    };
    nix2container.url = "github:nlewo/nix2container";
  };

  outputs = inputs@{ self, flake-parts, fenix, foundry, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;
      imports = [
        ./nix/lib
        ./nix/packages
        ./nix/ci.nix
        flake-parts.flakeModules.flakeModules
        flake-parts.flakeModules.easyOverlay
      ];
      perSystem = { pkgs, system, ... }:
        let
          pkgConfig = {
            inherit system;
            # Rust Overlay
            overlays = [
              fenix.overlays.default
              foundry.overlay
              self.overlays.default
            ];
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
            default = pkgs.callPackage ./nix/libxmtp.nix { };
            # Shell for android builds
            android = pkgs.callPackage ./nix/android.nix { };
            # Shell for iOS builds
            ios = pkgs.callPackage ./nix/ios.nix { };
            js = pkgs.callPackage ./nix/js.nix { };
          };
        };
    };
}
