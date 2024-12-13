# Flake Shell for building release artifacts for swift and kotlin
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    fenix = {
      url = "github:nix-community/fenix";
      inputs = { nixpkgs.follows = "nixpkgs"; };
    };
    systems.url = "github:nix-systems/default";
    # A nix-native packaging system for rust (docs: https://crane.dev/index.html)
    crane.url = "github:ipetkov/crane";

    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  nixConfig = {
    extra-trusted-public-keys = "xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0=";
    extra-substituters = "https://xmtp.cachix.org";
  };

  outputs = inputs@{ flake-parts, fenix, crane, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;
      perSystem = { pkgs, lib, inputs', system, ... }:
        let
          fenixPkgs = inputs'.fenix.packages;
          androidTargets = [
            "aarch64-linux-android"
            "armv7-linux-androideabi"
            "x86_64-linux-android"
            "i686-linux-android"
          ];

          # Pinned Rust Version
          rust-toolchain = with fenix.packages.${system}; combine [
            stable.cargo
            stable.rustc
            (pkgs.lib.forEach
              androidTargets
              (target: targets."${target}".stable.rust-std))
            targets.x86_64-unknown-linux-musl.stable.rust-std
          ];

          pkgConfig = {
            inherit system;
            overlays = [ fenix.overlays.default ];
            config = {
              android_sdk.accept_license = true;
              allowUnfree = true;
            };
          };

          # Library to build rust crates & test them
          # Set the rust toolchain package/versioning
          craneLib = (crane.mkLib pkgs).overrideToolchain (p: rust-toolchain);

          inherit (craneLib.fileset) commonCargoSources;

          libFileSetForWorkspace = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            (commonCargoSources ./xmtp_api_grpc)
            (commonCargoSources ./xmtp_api_http)
            (commonCargoSources ./xmtp_cryptography)
            (commonCargoSources ./xmtp_id)
            (commonCargoSources ./xmtp_mls)
            (commonCargoSources ./xmtp_proto)
            (commonCargoSources ./xmtp_v2)
            (commonCargoSources ./xmtp_user_preferences)
            (commonCargoSources ./common)
            (commonCargoSources ./xmtp_content_types)
            ./xmtp_id/src/scw_verifier/chain_urls_default.json
            ./xmtp_id/artifact
            ./xmtp_mls/migrations
          ];
          binFileSetForWorkspace = lib.fileset.unions [
            (commonCargoSources ./examples/cli)
            (commonCargoSources ./mls_validation_service)
            (commonCargoSources ./bindings_node)
            (commonCargoSources ./bindings_wasm)
            (commonCargoSources ./xtask)
            (commonCargoSources ./bindings_ffi)
            (commonCargoSources ./xmtp_debug)
          ];
          fileSetForCrate = crate: lib.fileset.unions [
            libFileSetForWorkspace
            crate
          ];
          fileSetForWorkspace = lib.fileset.unions [
            binFileSetForWorkspace
            libFileSetForWorkspace
          ];
          filesets = {
            inherit
              fileSetForWorkspace
              binFileSetForWorkspace
              libFileSetForWorkspace
              fileSetForCrate;
          };

          xdbg = import ./nix/xdbg {
            inherit pkgs craneLib filesets;
          };
          validationService = import ./nix/mls_validation_service {
            inherit pkgs craneLib filesets;
          };
        in
        {
          _module.args.pkgs = import inputs.nixpkgs pkgConfig;
          devShells.android = pkgs.callPackage ./nix/android.nix { inherit rust-toolchain; };
          packages = {
            xdbg = xdbg.bin;
            xdbgDocker = xdbg.dockerImage;
            validationService = validationService.bin;
            validationServiceDocker = validationService.dockerImage;
          };
        };
    };
}
