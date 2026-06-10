# Flake Shell for building release artifacts for swift and kotlin
{
  nixConfig = {
    http-connections = 128;
    max-substitution-jobs = 128;
    sandbox = "relaxed";
  };

  inputs = {
    # Cross pkgsets apply the iOS branch (NixOS/nixpkgs#512100) as a patch
    # on top — see nixpkgs-patched in nix/lib/default.nix.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
    };
    foundry = {
      url = "github:shazow/foundry.nix/stable";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane = {
      url = "github:ipetkov/crane";
    };
    rust-manifest = {
      url = "https://static.rust-lang.org/dist/channel-rust-1.95.0.toml";
      flake = false;
    };
    treefmt-nix.url = "github:numtide/treefmt-nix";
  };

  outputs =
    inputs@{ flake-parts, self, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "aarch64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ];
      imports = [
        ./nix/lib
        flake-parts.flakeModules.easyOverlay
        inputs.treefmt-nix.flakeModule
        ./nix/musl-docker.nix
        ./nix/ci-checks.nix
        ./nix/fmt.nix
        ./nix/node-packages.nix
        ./nix/android-packages.nix
        ./nix/apps.nix
        ./nix/ios-packages.nix
      ];
      perSystem =
        {
          pkgs,
          lib,
          self',
          system,
          ...
        }:
        {
          _module.args.pkgs = lib.mkForce (self.lib.mkXmtpPkgs { inherit system; });
          overlayAttrs = {
            inherit (self'.packages) xnet-cli;
          };
          devShells = {
            rust = pkgs.callPackage ./nix/shells/rust.nix { };
            default = pkgs.callPackage ./nix/shells/local.nix { };
            android = pkgs.callPackage ./nix/shells/android.nix { };
            js = pkgs.callPackage ./nix/js.nix { };
            wasm = (pkgs.callPackage ./nix/package/wasm.nix { }).devShell;
          }
          // lib.optionalAttrs pkgs.stdenv.isDarwin {
            ios = pkgs.callPackage ./nix/shells/ios.nix { };
          };
          packages = {
            inherit (pkgs.xmtp)
              ffi-uniffi-bindgen
              xdbg-driver-lib
              cross-talk-test
              cross-version-test
              ;
            inherit (pkgs)
              napi-rs-cli
              wasm-bindgen-cli
              ;
            wasm-bindings = (pkgs.callPackage ./nix/package/wasm.nix { }).bin;
            wasm-bindings-test = (pkgs.callPackage ./nix/package/wasm.nix { test = true; }).bin;
          };
        };
    };
}
