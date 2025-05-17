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
        combineShell =
          { otherShells
          , extraInputs
          , stdenv ? pkgs.stdenv
          }: (pkgs.callPackage ./combineShell.nix {
            inherit otherShells extraInputs stdenv;
          });
        shells = {
          mkLinters = pkgs.callPackage ./mkLinters.nix { };
          mkGrpc = pkgs.callPackage ./mkGrpc.nix { };
          mkRustWasm = pkgs.callPackage ./mkRustWasm.nix { };
          mkCargo = pkgs.callPackage ./mkCargo.nix { };
        };
        mkWorkspace = import ./mkWorkspace.nix;
        filesets = import ./filesets.nix;
      };
    };
  };
}

