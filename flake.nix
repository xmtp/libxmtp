# Flake Shell for building release artifacts for swift and kotlin
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs = { nixpkgs.follows = "nixpkgs"; };
    };
    flake-parts = { url = "github:hercules-ci/flake-parts"; };
    systems.url = "github:nix-systems/default";
    mkshell-util.url = "github:insipx/mkShell-util.nix";
    foundry.url = "github:shazow/foundry.nix/monthly";
    crane = {
      url = "github:ipetkov/crane";
    };
    rust-manifest = {
      url = "https://static.rust-lang.org/dist/channel-rust-stable.toml";
      flake = false;
    };
  };

  nixConfig = {
    extra-trusted-public-keys = "xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0=";
    extra-substituters = "https://xmtp.cachix.org";
  };

  outputs = inputs@{ flake-parts, fenix, crane, foundry, rust-manifest, ... }:
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
            overlays = [ fenix.overlays.default foundry.overlay ];
            config = {
              android_sdk.accept_license = true;
              allowUnfree = true;
            };
          };
          mkToolchain = targets: components: pkgs.fenix.combine [
            ((pkgs.fenix.fromManifestFile rust-manifest).minimalToolchain)
            (pkgs.lib.forEach targets (target: (pkgs.fenix.targets."${target}".fromManifestFile rust-manifest).rust-std))
            (pkgs.lib.forEach components (c: (pkgs.fenix.fromManifestFile rust-manifest)."${c}"))
          ];
          craneLib = crane.mkLib pkgs;
          filesets = pkgs.callPackage ./nix/filesets.nix { inherit craneLib; };
        in
        {
          _module.args.pkgs = import inputs.nixpkgs pkgConfig;
          devShells = {
            # shell for general xmtp rust dev
            default = callPackage pkgs ./nix/libxmtp.nix { inherit mkToolchain; };
            # Shell for android builds
            android = callPackage pkgs ./nix/android.nix { inherit mkToolchain; };
            # Shell for iOS builds
            ios = callPackage pkgs ./nix/ios.nix { inherit mkToolchain; };
            js = callPackage pkgs ./nix/js.nix { };
            # the environment bindings_wasm is built in
            wasmBuild = (callPackage pkgs ./nix/package/bindings_wasm.nix { inherit filesets; craneLib = crane.mkLib pkgs; }).devShell;
          };
          packages.bindings_wasm = (pkgs.callPackage ./nix/package/bindings_wasm.nix { inherit filesets; craneLib = crane.mkLib pkgs; }).bin;
        };
    };
}
