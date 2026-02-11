{ inputs, self, ... }: {
  flake.lib = {
    pkgConfig = {
      # Rust Overlay
      overlays = [ inputs.fenix.overlays.default inputs.foundry.overlay self.overlays.default ];
      config = {
        android_sdk.accept_license = true;
        allowUnfree = true;
      };
    };
  };
  perSystem = { pkgs, ... }: {
    overlayAttrs = {
      xmtp = {
        mkToolchain = pkgs.callPackage ./mkToolchain.nix { inherit inputs; };
        filesets = pkgs.callPackage ./filesets.nix { };
        craneLib = inputs.crane.mkLib pkgs;
        mobile = pkgs.callPackage ./mobile-common.nix { };
        androidEnv = pkgs.callPackage ./android-env.nix { };
        iosEnv = pkgs.callPackage ./ios-env.nix { };
      };
      wasm-bindgen-cli = pkgs.callPackage ./packages/wasm-bindgen-cli.nix { };
      swiftformat = pkgs.callPackage ./packages/swiftformat.nix { };
      swiftlint = pkgs.callPackage ./packages/swiftlint.nix { };
    };
  };
}
