# Flake Shell for building release artifacts for swift and kotlin
{
  nixConfig = {
    http-connections = 128;
    max-substitution-jobs = 128;
    sandbox = "relaxed";
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
    };
    foundry.url = "github:shazow/foundry.nix/stable";
    crane = {
      url = "github:ipetkov/crane";
    };
    rust-manifest = {
      url = "https://static.rust-lang.org/dist/channel-rust-1.97.1.toml";
      flake = false;
    };
    treefmt-nix.url = "github:numtide/treefmt-nix";
    # The test services (nix/services). The project has no release after
    # 0.1.0, so pin a revision.
    process-compose-flake.url = "github:Platonic-Systems/process-compose-flake/142d4b084026e91d9ab5822639d308aca5fb1b2b";
    # The bindgen fork. Its revision must equal Cargo.toml
    # [workspace.metadata.xmtp-sdk-fork]; `nix/lib/packages/ubrn.nix` checks it.
    ubrn-src = {
      url = "github:neekolas/uniffi-bindgen-react-native/c4ae748b187807e6da4f9c841ad83838be7f12f2";
      flake = false;
    };
  };

  outputs =
    inputs@{ flake-parts, self, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "aarch64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ];
      imports = [
        ./nix/lib
        inputs.treefmt-nix.flakeModule
        ./nix/musl-docker.nix
        ./nix/ci-checks.nix
        ./nix/ci-sets.nix
        ./nix/fmt.nix
        ./nix/node-packages.nix
        ./nix/android-packages.nix
        ./nix/react-native-packages.nix
        ./nix/apps.nix
        ./nix/services
      ];
      perSystem =
        {
          pkgs,
          lib,
          system,
          ...
        }:
        let
          sdk = pkgs.callPackage ./nix/package/xmtp-sdk.nix { };
          # stdenvNoCC avoids Nix's apple-sdk and cc-wrapper, which inject
          # -mmacos-version-min flags that conflict with iOS cross-compilation.
          # The builds are impure (__noChroot) and use the system Xcode SDK
          # through ios-env.nix.
          ios = pkgs.callPackage ./nix/package/ios.nix {
            stdenv = pkgs.stdenvNoCC;
            inherit sdk;
          };
          iosLocal = pkgs.callPackage ./nix/package/ios.nix {
            stdenv = pkgs.stdenvNoCC;
            inherit sdk;
            local = true;
          };
        in
        {
          _module.args.pkgs = lib.mkForce (self.lib.mkXmtpPkgs { inherit system; });
          # The SDK closure identities (`nix/lib/sdk-sources.nix`). The cargo
          # route reads them with `nix eval .#sdkProvenance`, so both routes
          # record the same values.
          legacyPackages.sdkProvenance = sdk.provenance;
          apps = lib.optionalAttrs pkgs.stdenv.isDarwin {
            example-ios = {
              type = "app";
              program = "${pkgs.callPackage ./nix/package/example-ios-run.nix { }}/bin/example-ios";
            };
          };
          devShells =
            let
              # The Rust core; `default` layers on it, and `android` on its core
              # packages and environment.
              rustShell = pkgs.callPackage ./nix/shells/rust.nix { };
              default = pkgs.callPackage ./nix/shells/local.nix { inherit rustShell; };
            in
            {
              rust = rustShell;
              inherit default;
              android = pkgs.callPackage ./nix/shells/android.nix { };
              js = pkgs.callPackage ./nix/js.nix { };
              js-node = pkgs.callPackage ./nix/js-node.nix { };
              react-native = pkgs.callPackage ./nix/shells/react-native.nix { };
              react-native-local = pkgs.callPackage ./nix/shells/react-native.nix { local = true; };
              docs = pkgs.callPackage ./nix/docs.nix { };
              # Old names: `wasm` is the Rust core, `ios` the default shell.
              wasm = rustShell;
            }
            // lib.optionalAttrs pkgs.stdenv.isDarwin {
              ios = default;
            }
            # The Android CI jobs run on Linux.
            // lib.optionalAttrs pkgs.stdenv.isLinux {
              android-ci = pkgs.callPackage ./nix/shells/android-ci.nix { };
              android-ci-emulator = pkgs.callPackage ./nix/shells/android-ci.nix { emulator = true; };
            };
          packages = {
            kache = pkgs.kache;
            xmtp-sdk-libs = sdk.libs;
            xmtp-sdk-pure-wasm = sdk.pureWasm;
            xmtp-sdk-wasm = sdk.wasm;
            xmtp-sdk-bindgen = sdk.bindgen;
            xmtp-sdk-generated = sdk.generated;
            xmtp-sdk-generated-swift = sdk.generatedSwift;
            xmtp-sdk-generated-kotlin = sdk.generatedKotlin;
            xmtp-sdk-generated-node = sdk.generatedNode;
            xmtp-sdk-generated-react-native = sdk.generatedReactNative;
            xmtp-sdk-generated-browser = sdk.generatedBrowser;
            xmtp-sdk-wasm-fixture-panic = sdk.panicFixture;
            xmtp-sdk-pure-fixture-codec = sdk.codecFixture;
            xmtp-legacy-migration-fixture-host = sdk.migrationFixtureHost;
            ubjs-core = pkgs.xmtp.ubrn.core;
            ubjs-react-native = pkgs.xmtp.ubrn.reactNative;
            ubjs-node = pkgs.xmtp.ubrn.node;
            ubjs-wasm = pkgs.xmtp.ubrn.wasm;
            inherit (pkgs)
              napi-rs-cli
              wasm-bindgen-cli
              ;
          }
          // lib.optionalAttrs pkgs.stdenv.isDarwin {
            xmtp-sdk-ios-device = sdk.iosTargets.aarch64-apple-ios;
            xmtp-sdk-ios-simulator = sdk.iosTargets.aarch64-apple-ios-sim;
            ios-xcframeworks = ios.release;
            ios-xcframeworks-fast = ios.devFast;
            # For a Mac whose Xcode is not the CI Xcode (`lib.xcode`):
            # sdks/ios/dev/bindings selects these. Caches do not hold them.
            ios-xcframeworks-local = iosLocal.release;
            ios-xcframeworks-fast-local = iosLocal.devFast;
          };
        };
    };
}
