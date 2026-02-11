# Flake Shell for building release artifacts for swift and kotlin
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs = { nixpkgs.follows = "nixpkgs"; };
    };
    flake-parts = { url = "github:hercules-ci/flake-parts"; };
    foundry.url = "github:shazow/foundry.nix/stable";
    crane = {
      url = "github:ipetkov/crane";
    };
    rust-flake.url = "github:juspay/rust-flake";
    rust-manifest = {
      url = "https://static.rust-lang.org/dist/channel-rust-1.92.0.toml";
      flake = false;
    };
  };

  nixConfig = {
    extra-trusted-public-keys = [
      "xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0="
      "nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs="
    ];
    extra-substituters = [
      "https://xmtp.cachix.org"
      "https://nix-community.cachix.org"
    ];
  };

  outputs =
    inputs @ { flake-parts, self, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "aarch64-darwin"
        "x86_64-linux"
      ];
      imports = [
        ./nix/lib
        flake-parts.flakeModules.easyOverlay
        inputs.rust-flake.flakeModules.default
        inputs.rust-flake.flakeModules.nixpkgs
        ./nix/rust-defaults.nix
        ./nix/rust.nix
      ];
      perSystem =
        { pkgs, lib, self', ... }: {
          nixpkgs = self.lib.pkgConfig;
          devShells = {
            # shell for general xmtp rust dev
            default = pkgs.callPackage ./nix/libxmtp.nix { };
            # Shell for android builds
            android = pkgs.callPackage ./nix/android.nix { };
            js = pkgs.callPackage ./nix/js.nix { };
            # the environment bindings_wasm is built in
            wasm = (pkgs.callPackage ./nix/package/wasm.nix { }).devShell;
          } // lib.optionalAttrs pkgs.stdenv.isDarwin {
            # Shell for iOS builds
            ios = pkgs.callPackage ./nix/ios.nix { };
          };
          packages =
            let
              android = pkgs.callPackage ./nix/package/android.nix { };
              inherit (pkgs.xmtp) androidEnv;
            in
            {
              inherit (pkgs.xmtp) ffi-uniffi-bindgen;
              wasm-bindings = (pkgs.callPackage ./nix/package/wasm.nix { }).bin;
              wasm-bindgen-cli = pkgs.callPackage ./nix/lib/packages/wasm-bindgen-cli.nix { };
              # Android bindings (.so libraries + Kotlin bindings)
              android-libs = android.aggregate;
              # Android bindings - host-matching target only (fast dev/CI builds)
              android-libs-fast = (android.mkAndroid [ androidEnv.hostAndroidTarget ]).aggregate;
              docker-mls_validation_service = pkgs.dockerTools.buildLayeredImage {
                name = "ghcr.io/xmtp/mls-validation-service"; # override ghcr images
                tag = "main";
                created = "now";
                config = {
                  Env = [
                    "ANVIL_URL=http://anvil:8545"
                  ];
                  entrypoint = [ "${self'.packages.musl-mls_validation_service}/bin/mls-validation-service" ];
                };
              };
            } // lib.optionalAttrs pkgs.stdenv.isDarwin {
              # stdenvNoCC is passed to both callPackage (for the aggregate derivation)
              # This avoids Nix's apple-sdk and cc-wrapper,
              # which inject -mmacos-version-min flags that
              # conflict with iOS cross-compilation. The builds are impure (__noChroot)
              # and use the system Xcode SDK directly via ios-env.nix paths.
              ios-libs = (pkgs.callPackage ./nix/package/ios.nix {
                stdenv = pkgs.stdenvNoCC;
              }).aggregate;
            };
        };
    };
}
