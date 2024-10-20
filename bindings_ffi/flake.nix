{
  description = "Rust cross-compilation setup for x86_64-linux-android";

  inputs = {
    # Include nixpkgs
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";
    flake-utils.url = "github:numtide/flake-utils";
    # Cross-overlay input
    nixpkgs-cross-overlay = {
      url = "github:alekseysidorov/nixpkgs-cross-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    fenix = {
      url = "github:nix-community/fenix";
      inputs = { nixpkgs.follows = "nixpkgs"; };
    };
  };

  outputs = { self, nixpkgs, flake-utils, fenix, nixpkgs-cross-overlay }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          overlays = [ (import nixpkgs-cross-overlay) ];
          pkgs = import nixpkgs {
            inherit system overlays crossSystem;
          };
          fenixPkgs = fenix.packages.${system};

          rust-toolchain = fenixPkgs.fromToolchainFile {
            file = ./../rust-toolchain;
            sha256 = "sha256-0000000000000000000000000000000000000000000=";
          };

          # Define the cross-compilation target
          crossSystem = {
            config = "x86_64-linux-android";
            useLLVM = true;
            isStatic = false; # Android usually needs dynamic linking
          };
          nativeBuildInputs = with pkgs; [
          # pkgsBuildHost.rust-bin.stable.latest.default
            pkgsBuildHost.rustBuildHostDependencies
            # Crates Dependencies

            pkgs.cargoDeps.openssl-sys
            # androidndk # Provides the Android NDK, essential for cross-compiling
            # cmake # Android builds often use cmake
            # pkg-config # Required to configure build dependencies
          ];

          buildInputs = with pkgs; [
            rustCrossHook
            openssl
            zlib
            icu
          ];

        shellHook = ''
          echo "Android cross-compilation environment setup for Rust (x86_64-linux-android)"
        '';
      in
        with pkgs;
        {
          devShells.default = mkShell {
            # 👇 and now we can just inherit them
            inherit buildInputs nativeBuildInputs shellHook;
          };
        }
      );
}
