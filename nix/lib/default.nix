{ inputs, self, ... }:
{
  flake.lib = {
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
  };
  perSystem =
    { pkgs, ... }:
    let
      craneConfig = final: prev: {
        # add napi builder to crane scope
        napiBuild = final.callPackage ./napiBuild.nix { };
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
        swiftformat = pkgs.callPackage ./packages/swiftformat.nix { };
        swiftlint = pkgs.callPackage ./packages/swiftlint.nix { };
      };
    };
}
