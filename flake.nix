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
    rust-manifest = {
      url = "https://static.rust-lang.org/dist/channel-rust-1.90.0.toml";
      flake = false;
    };
  };

  nixConfig = {
    extra-trusted-public-keys = "xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0=";
    extra-substituters = "https://xmtp.cachix.org";
  };

  outputs = inputs@{ flake-parts, fenix, crane, foundry, rust-manifest, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
      perSystem = { pkgs, system, inputs', ... }:
        let
          pkgConfig = {
            inherit system;
            # Rust Overlay
            overlays = [ fenix.overlays.default foundry.overlay ];
            config = {
              android_sdk.accept_license = true;
              allowUnfree = true;
            };
          };
          toolchain = (inputs'.fenix.packages.fromManifestFile rust-manifest).defaultToolchain;
          mkToolchain = targets: components: pkgs.fenix.combine [
            toolchain
            (pkgs.lib.forEach targets (target: (pkgs.fenix.targets."${target}".fromManifestFile rust-manifest).rust-std))
            (pkgs.lib.forEach components (component: (inputs'.fenix.packages.fromManifestFile rust-manifest)."${component}"))
          ];
          craneLib = crane.mkLib pkgs;
          filesets = pkgs.callPackage ./nix/filesets.nix { inherit craneLib; };
        in
        {
          _module.args.pkgs = import inputs.nixpkgs pkgConfig;
          devShells = {
            # shell for general xmtp rust dev
            default = pkgs.callPackage ./nix/libxmtp.nix { inherit mkToolchain; };
            # Shell for android builds
            android = pkgs.callPackage ./nix/android.nix { inherit mkToolchain; };
            # Shell for iOS builds
            ios = pkgs.callPackage ./nix/ios.nix { inherit mkToolchain; };
            js = pkgs.callPackage ./nix/js.nix { };
            # the environment bindings_wasm is built in
            wasmBuild = (pkgs.callPackage ./nix/package/bindings_wasm.nix { inherit filesets; craneLib = crane.mkLib pkgs; }).devShell;
          };
          packages.wasm-bindings = (pkgs.callPackage ./nix/package/bindings_wasm.nix { inherit filesets; craneLib = crane.mkLib pkgs; }).bin;
        };
    };
}
