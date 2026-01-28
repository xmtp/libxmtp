# Flake Shell for building release artifacts for swift and kotlin
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    fenix = {
      url = "github:nix-community/fenix";
      inputs = { nixpkgs.follows = "nixpkgs"; };
    };
    flake-parts = { url = "github:hercules-ci/flake-parts"; };
    foundry.url = "github:shazow/foundry.nix/stable";
    crane = {
      url = "github:ipetkov/crane";
    };
    rust-manifest = {
      url = "https://static.rust-lang.org/dist/channel-rust-1.92.0.toml";
      flake = false;
    };
  };

  nixConfig = {
    extra-trusted-public-keys = "xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0=";
    extra-substituters = "https://xmtp.cachix.org";
  };

  outputs =
    inputs @ { self
    , flake-parts
    , fenix
    , crane
    , foundry
    , ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "aarch64-darwin"
        "x86_64-linux"
      ];
      imports = [
        ./nix/lib
        flake-parts.flakeModules.easyOverlay
      ];
      perSystem =
        { pkgs
        , system
        , ...
        }:
        let
          pkgConfig = {
            inherit system;
            # Rust Overlay
            overlays = [ fenix.overlays.default foundry.overlay self.overlays.default ];
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
            # the environment bindings_wasm is built in
            wasm = (pkgs.callPackage ./nix/package/wasm.nix { craneLib = crane.mkLib pkgs; }).devShell;
          };
          packages.wasm-bindings = (pkgs.callPackage ./nix/package/wasm.nix { craneLib = crane.mkLib pkgs; }).bin;
          packages.wasm-bindgen-cli = pkgs.callPackage ./nix/lib/packages/wasm-bindgen-cli.nix { };

        };
    };
}
