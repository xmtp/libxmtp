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
    {
      overlayAttrs = {
        xmtp = {
          mkToolchain = pkgs.callPackage ./mkToolchain.nix { inherit inputs; };
          filesets = pkgs.callPackage ./filesets.nix { };
          craneLib = inputs.crane.mkLib pkgs;
          mobile = pkgs.callPackage ./mobile-common.nix { };
          androidEnv = pkgs.callPackage ./android-env.nix { };
          iosEnv = pkgs.callPackage ./ios-env.nix { };
          nodeEnv = pkgs.callPackage ./node-env.nix { };
          ffi-uniffi-bindgen = pkgs.callPackage ./packages/uniffi-bindgen.nix { };
          shellCommon = pkgs.callPackage ./shell-common.nix { };
        };
        wasm-bindgen-cli = pkgs.callPackage ./packages/wasm-bindgen-cli.nix { };
        swiftformat = pkgs.callPackage ./packages/swiftformat.nix { };
        swiftlint = pkgs.callPackage ./packages/swiftlint.nix { };
      };
    };
}
