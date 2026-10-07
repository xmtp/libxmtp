# Flake Shell for building release artifacts for swift and kotlin
{
  nixConfig = {
    http-connections = 128;
    max-substitution-jobs = 128;
    sandbox = "relaxed";
  };

  inputs = {
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
    foundry.url = "github:shazow/foundry.nix/stable";
    crane = {
      url = "github:ipetkov/crane";
    };
    rust-manifest = {
      url = "https://static.rust-lang.org/dist/channel-rust-1.97.1.toml";
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
          apps = lib.optionalAttrs pkgs.stdenv.isDarwin {
            backend-ci = {
              type = "app";
              program = "${self'.packages.backend-ci}/bin/backend-ci";
            };
          };
          devShells = {
            rust = pkgs.callPackage ./nix/shells/rust.nix { };
            default = pkgs.callPackage ./nix/shells/local.nix { };
            android = pkgs.callPackage ./nix/shells/android.nix { };
            js = pkgs.callPackage ./nix/js.nix { };
            js-node = pkgs.callPackage ./nix/js-node.nix { };
            docs = pkgs.callPackage ./nix/docs.nix { };
            wasm = pkgs.callPackage ./nix/shells/wasm.nix { };
          }
          // lib.optionalAttrs pkgs.stdenv.isDarwin {
            ios = pkgs.callPackage ./nix/shells/ios.nix { };
          };
          packages = {
            kache = pkgs.kache;
            xmtp-sdk-libs = (pkgs.callPackage ./nix/package/xmtp-sdk.nix { }).libs;
            xmtp-sdk-pure-wasm = (pkgs.callPackage ./nix/package/xmtp-sdk.nix { }).pureWasm;
            xmtp-sdk-wasm = (pkgs.callPackage ./nix/package/xmtp-sdk.nix { }).wasm;
            xmtp-sdk-bindgen = (pkgs.callPackage ./nix/package/xmtp-sdk.nix { }).bindgen;
            xmtp-sdk-generated = (pkgs.callPackage ./nix/package/xmtp-sdk.nix { }).generated;
            xmtp-sdk-generated-swift = (pkgs.callPackage ./nix/package/xmtp-sdk.nix { }).generatedSwift;
            xmtp-sdk-generated-kotlin = (pkgs.callPackage ./nix/package/xmtp-sdk.nix { }).generatedKotlin;
            xmtp-sdk-generated-node = (pkgs.callPackage ./nix/package/xmtp-sdk.nix { }).generatedNode;
            xmtp-sdk-generated-browser = (pkgs.callPackage ./nix/package/xmtp-sdk.nix { }).generatedBrowser;
            ubrn = (pkgs.callPackage ./nix/lib/packages/ubrn.nix { }).cli;
            ubjs-core = (pkgs.callPackage ./nix/lib/packages/ubrn.nix { }).core;
            ubjs-node = (pkgs.callPackage ./nix/lib/packages/ubrn.nix { }).node;
            ubjs-wasm = (pkgs.callPackage ./nix/lib/packages/ubrn.nix { }).wasm;
            inherit (pkgs)
              napi-rs-cli
              wasm-bindgen-cli
              ;
          }
          // lib.optionalAttrs pkgs.stdenv.isDarwin {
            xmtp-sdk-ios-device =
              (pkgs.callPackage ./nix/package/xmtp-sdk.nix { }).iosTargets.aarch64-apple-ios;
            xmtp-sdk-ios-simulator =
              (pkgs.callPackage ./nix/package/xmtp-sdk.nix { }).iosTargets.aarch64-apple-ios-sim;
            # stdenvNoCC is passed to callPackage (for the aggregate derivation).
            # This avoids Nix's apple-sdk and cc-wrapper,
            # which inject -mmacos-version-min flags that
            # conflict with iOS cross-compilation. The builds are impure (__noChroot)
            # and use the system Xcode SDK directly via ios-env.nix paths.
            backend-ci = pkgs.callPackage ./nix/package/backend-ci.nix {
              backend = self'.packages.xmtp-backend;
            };
            ios-libs =
              (pkgs.callPackage ./nix/package/ios.nix {
                stdenv = pkgs.stdenvNoCC;
              }).aggregate;
            # iOS bindings - simulator + host macOS only (fast dev/CI builds)
            ios-libs-fast =
              (
                (pkgs.callPackage ./nix/package/ios.nix {
                  stdenv = pkgs.stdenvNoCC;
                }).mkIos
                [
                  "aarch64-apple-darwin"
                  "aarch64-apple-ios-sim"
                ]
              ).aggregate;
            ios-xcframeworks =
              (pkgs.callPackage ./nix/package/ios.nix {
                stdenv = pkgs.stdenvNoCC;
              }).release;
            ios-xcframeworks-fast =
              (pkgs.callPackage ./nix/package/ios.nix {
                stdenv = pkgs.stdenvNoCC;
              }).devFast;
          };
        };
    };
}
