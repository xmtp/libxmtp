# Flake Shell for building release artifacts for swift and kotlin
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";

    fenix = {
      url = "github:nix-community/fenix";
      inputs = { nixpkgs.follows = "nixpkgs"; };
    };

    flake-utils = { url = "github:numtide/flake-utils"; };
  };

  outputs = { nixpkgs, flake-utils, fenix, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        inherit (pkgs.stdenv) isDarwin;
        inherit (pkgs) androidenv;
        inherit (pkgs.darwin.apple_sdk) frameworks;
        inherit (androidComposition) androidsdk;
        pkgs = import nixpkgs {
          inherit system;
          # Rust Overlay
          overlays = [ fenix.overlays.default ];
          config = {
            android_sdk.accept_license = true;
            allowUnfree = true;
          };
        };

        android = {
          platforms = [ "34" ];
          platformTools = "33.0.3";
          buildTools = [ "30.0.3" ];
        };

        sdkArgs = {
          platformVersions = android.platforms;
          platformToolsVersion = android.platformTools;
          buildToolsVersions = android.buildTools;
          includeNDK = true;
        };

        fenixPkgs = fenix.packages.${system};
        # Pinned Rust Version
        rust-toolchain = fenixPkgs.fromToolchainFile {
          file = ./rust-toolchain;
          sha256 = "sha256-yMuSb5eQPO/bHv+Bcf/US8LVMbf/G/0MSfiPwBhiPpk=";
        };

        # https://github.com/NixOS/nixpkgs/blob/master/doc/languages-frameworks/android.section.md
        androidHome = "${androidComposition.androidsdk}/libexec/android-sdk";
        androidComposition = androidenv.composeAndroidPackages sdkArgs;
        nativeBuildInputs = with pkgs; [ pkg-config androidsdk jdk17 ];

        # Define the packages available to the build environment
        # https://search.nixos.org/packages
        buildInputs = with pkgs; [
          rust-toolchain
          kotlin
          cargo-ndk
          androidsdk
          jdk17

          # System Libraries
          sqlite
          openssl
        ] ++ lib.optionals isDarwin [ # optional packages if on darwin, in order to check if build passes locally
          libiconv
          frameworks.CoreServices
          frameworks.Carbon
          frameworks.ApplicationServices
          frameworks.AppKit
          darwin.cctools
        ];
      in {
        devShells.default = pkgs.mkShell {
            OPENSSL_DIR = "${pkgs.openssl.dev}";
            ANDROID_HOME = androidHome;
            ANDROID_SDK_ROOT = androidHome; # ANDROID_SDK_ROOT is deprecated, but some tools may still use it;
            ANDROID_NDK_ROOT = "${androidHome}/ndk-bundle";

            inherit buildInputs nativeBuildInputs;
          };
      });
}
