{ inputs, ... }: {
  flake.lib = {
    pkgConfig = {
      # Rust Overlay
      overlays = [ inputs.fenix.overlays.default inputs.foundry.overlay ];
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
        filesets = import ./filesets.nix;
      };
      wasm-bindgen-cli = pkgs.callPackage ./packages/wasm-bindgen-cli.nix { };
    };
  };
}
