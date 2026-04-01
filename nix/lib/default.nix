{
  inputs,
  self,
  lib,
  ...
}:
{
  flake.lib =
    let
      normalize =
        x:
        if builtins.isString x then
          { config = x; }
        else if builtins.isAttrs x then
          x
        else
          throw "expected a string or attribute set";
    in
    {
      pkgConfig = {
        # Rust Overlay
        overlays = [
          inputs.fenix.overlays.default
          inputs.foundry.overlay
          self.overlays.default
          # mold is significantly faster on linux for local dev
          (
            final: prev:
            prev.lib.optionalAttrs prev.stdenv.isLinux {
              mkShell = prev.mkShell.override {
                stdenv = prev.stdenvAdapters.useMoldLinker prev.clangStdenv;
              };
            }
          )
        ];
        config = {
          android_sdk.accept_license = true;
          allowUnfree = true;
        };
      };
      mkCrossPkgs =
        system: targets:
        let
          # Create pkgs for the build system to use applyPatches
          buildPkgs = import inputs.nixpkgs {
            inherit system;
            inherit (self.lib.pkgConfig) config;
          };
          # Apply Android NDK aarch64-darwin patch
          nixpkgs-patched = buildPkgs.applyPatches {
            name = "android-darwin-patch";
            src = inputs.nixpkgs;
            # can remove this patch once pull/505820 is merged into nixpkgs
            patches = [
              (buildPkgs.fetchpatch2 {
                url = "https://github.com/NixOS/nixpkgs/pull/505820.patch";
                sha256 = "sha256-1iEujs0metq+Q5dZc2yEzEdTdkQjntGaaBKW7WXwrAs=";
              })
            ];
          };
        in
        lib.listToAttrs (
          map (
            target:
            let
              t = normalize target;
            in
            {
              name = t.config;
              value = import nixpkgs-patched (
                self.lib.pkgConfig
                // {
                  localSystem = system;
                  crossSystem = t;
                }
              );
            }
          ) targets
        );
    };
  perSystem =
    {
      pkgs,
      ...
    }:
    let
      craneConfig = final: prev: {
        # add napi builder to crane scope
        napiBuild = final.callPackage ./napiBuild.nix { };
        uniffiGenerate = final.callPackage ./uniffiGenerate.nix { };
      };
      mkToolchain = pkgs.callPackage ./mkToolchain.nix { inherit inputs; };
    in
    {
      overlayAttrs = {
        xmtp = {
          inherit mkToolchain;
          # toolchain with native pkgs
          mkNativeToolchain = mkToolchain pkgs;
          filesets = pkgs.callPackage ./filesets.nix { };
          craneLib = (inputs.crane.mkLib pkgs).overrideScope craneConfig;
          base = pkgs.callPackage ./base.nix { };
          androidEnv = pkgs.callPackage ./android-env.nix { };
          iosEnv = pkgs.callPackage ./ios-env.nix { };
          ffi-uniffi-bindgen = pkgs.callPackage ./packages/uniffi-bindgen.nix { };
          shellCommon = pkgs.callPackage ./shell-common.nix { };
          mkVersion = import ./mkVersion.nix;
          toNapiTarget = import ./napiTarget.nix;
        };
        wasm-bindgen-cli = pkgs.callPackage ./packages/wasm-bindgen-cli.nix { };
        napi-rs-cli = pkgs.callPackage ./packages/napi-rs-cli { };
        ffi-uniffi-bindgen = pkgs.callPackage ./packages/uniffi-bindgen.nix { };
        swiftformat = pkgs.callPackage ./packages/swiftformat.nix { };
        swiftlint = pkgs.callPackage ./packages/swiftlint.nix { };
      };
    };
}
