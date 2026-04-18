# Flake Shell for building release artifacts for swift and kotlin
{
  nixConfig = {
    http-connections = 128;
    max-substitution-jobs = 128;
    sandbox = "relaxed";
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # nixpkgs.url = "github:insipx/nixpkgs/insipx/ios-build";
    # nixpkgs.url = "git+file:///Users/insipx/code/nixpkgs";
    # nixpkgs.url = "git+file:///home/insipx/code/nixpkgs";
    # nixpkgs.url = "git+file:///workspace/nixpkgs?ref=insipx/ios-build";
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
          # // lib.optionalAttrs pkgs.stdenv.isDarwin {
          #   # stdenvNoCC is passed to callPackage (for the aggregate derivation).
          #   # This avoids Nix's apple-sdk and cc-wrapper,
          #   # which inject -mmacos-version-min flags that
          #   # conflict with iOS cross-compilation. The builds are impure (__noChroot)
          #   # and use the system Xcode SDK directly via ios-env.nix paths.
          #   ios-libs =
          #     (pkgs.callPackage ./nix/package/ios.nix {
          #       stdenv = pkgs.stdenvNoCC;
          #     }).aggregate;
          #   # iOS bindings - simulator + host macOS only (fast dev/CI builds)
          #   ios-libs-fast =
          #     (
          #       (pkgs.callPackage ./nix/package/ios.nix {
          #         stdenv = pkgs.stdenvNoCC;
          #       }).mkIos
          #       [
          #         "aarch64-apple-darwin"
          #         "aarch64-apple-ios-sim"
          #       ]
          #     ).aggregate;
          #   ios-xcframeworks =
          #     (pkgs.callPackage ./nix/package/ios.nix {
          #       stdenv = pkgs.stdenvNoCC;
          #     }).release;
          #   ios-xcframeworks-fast =
          #     (pkgs.callPackage ./nix/package/ios.nix {
          #       stdenv = pkgs.stdenvNoCC;
          #     }).devFast;
          # };
        };
    };
}
