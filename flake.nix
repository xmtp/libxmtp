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
    mvn2nix.url = "github:fzakaria/mvn2nix";
  };

  outputs = inputs@{ self, flake-parts, fenix, crane, foundry, rust-manifest, mvn2nix, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;
      imports = [
        ./nix/lib
        flake-parts.flakeModules.flakeModules
      ];
      perSystem = { pkgs, system, inputs', ... }:
        let
          util = import inputs.mkshell-util;
          mkShellWrappers = pkgs: util callPackage pkgs;
          callPackage = pkgs: pkgs.lib.callPackageWith ((mkShellWrappers pkgs) // pkgs);
          pkgConfig = {
            inherit system;
            # Rust Overlay
            overlays = [ fenix.overlays.default foundry.overlay mvn2nix.overlay ];
            config = {
              android_sdk.accept_license = true;
              allowUnfree = true;
            };
          };
          toolchain = (inputs'.fenix.packages.fromManifestFile rust-manifest).defaultToolchain;
          mkToolchain = targets: components: pkgs.fenix.combine [
            toolchain
            (pkgs.lib.forEach targets (target: (pkgs.fenix.targets."${target}".fromManifestFile rust-manifest).rust-std))
            (pkgs.lib.forEach components (c: (inputs'.fenix.packages.fromManifestFile rust-manifest)."${c}"))
          ];
          filesets = self.lib.filesets { inherit pkgs inputs; };
        in
        {
          _module.args.pkgs = import inputs.nixpkgs pkgConfig;
          devShells = {
            # shell for general xmtp rust dev
            default = callPackage pkgs ./nix/libxmtp.nix { inherit mkToolchain; };
            # shell for general xmtp rust dev
            ci = callPackage pkgs ./nix/ci.nix { inherit mkToolchain; };

            # Shell for android builds
            android = callPackage pkgs ./nix/android.nix { inherit mkToolchain; };
            # Shell for iOS builds
            ios = callPackage pkgs ./nix/ios.nix { inherit mkToolchain; };
            js = callPackage pkgs ./nix/js.nix { };
            kotlin = callPackage pkgs ./nix/kotlin.nix { inherit mkToolchain; };
            # the environment bindings_wasm is built in
            wasmBuild = (callPackage pkgs ./nix/package/bindings_wasm.nix { inherit filesets; craneLib = crane.mkLib pkgs; }).devShell;
          };
          packages = {
            bindingsWasm = (pkgs.callPackage ./nix/package/bindings_wasm.nix { inherit filesets; craneLib = crane.mkLib pkgs; }).bin;
            validationService = (pkgs.callPackage ./nix/package/mls_validation_service { inherit filesets mkToolchain; craneLib = crane.mkLib pkgs; }).bin;
            validationServiceDocker = (pkgs.callPackage ./nix/package/mls_validation_service { inherit filesets mkToolchain; craneLib = crane.mkLib pkgs; }).dockerImage;
          };
        };
    };
}
