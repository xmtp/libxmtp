# Warm sets: the outputs that .github/workflows/post-merge.yml builds, pushes to the
# trusted Cachix cache `xmtp`, and pins on every push to trunk.
#
# `ciSets.<system>.dev` holds what developers and CI jobs use;
# `ciSets.<system>.release` holds what release jobs use. Each set is a link
# farm, so one store path holds the runtime closure of every member, and
# `passthru.members` lists the members for the closure check of post-merge.yml.
#
# A dev shell enters as its `inputDerivation`, whose runtime closure holds
# every shell input. Add a member only when a job, a release, or a developer
# builds that output from trunk.
#
# Every other set is a product group of .github/ci-suites.yml: CI builds
# `ciSets.x86_64-linux.<group>` once per run in the job `products_<group>`.
# post-merge.yml builds only dev and release, so keep each group member in
# one of them; its closure check covers every set.
{ lib, withSystem, ... }:
let
  # Every dev shell of the system, as `shell-<name>`.
  shells =
    self':
    lib.mapAttrs' (
      name: shell: lib.nameValuePair "shell-${name}" shell.inputDerivation
    ) self'.devShells;

  mkSet =
    pkgs: name: members:
    (pkgs.linkFarm "ci-set-${name}" members).overrideAttrs {
      passthru = { inherit members; };
    };

  rnDev =
    pkgs: p:
    {
      inherit (p)
        react-native-js-tools
        react-native-js-tools-deps
        react-native-consumer-deps-bare
        react-native-consumer-deps-expo
        react-native-consumer-template-bare
        react-native-consumer-template-expo
        ubjs-react-native
        xmtp-sdk-generated-react-native
        xmtp-sdk-react-native-js
        ;
    }
    // lib.optionalAttrs pkgs.stdenv.isLinux {
      inherit (p)
        xmtp-sdk-react-native-android-fast
        react-native-consumer-android
        react-native-consumer-native-deps-android
        ;
    }
    // lib.optionalAttrs pkgs.stdenv.isDarwin {
      inherit (p)
        xmtp-sdk-react-native-ios-simulator
        react-native-consumer-ios
        react-native-consumer-native-deps-ios
        ;
    };

  rnRelease =
    pkgs: p:
    if pkgs.stdenv.isLinux then
      {
        inherit (p) xmtp-sdk-react-native-android xmtp-sdk-react-native-package;
      }
    else
      {
        inherit (p) xmtp-sdk-react-native-ios;
      };

  sets = {
    x86_64-linux =
      { pkgs, self', ... }:
      let
        p = self'.packages;
      in
      rec {
        dev = mkSet pkgs "dev" (baseDev.passthru.members // rnDev pkgs p);
        release = mkSet pkgs "release" (baseRelease.passthru.members // rnRelease pkgs p);
        baseDev = mkSet pkgs "base-dev" (
          shells self'
          // {
            inherit (p)
              xmtp-sdk-libs
              android-sdk-libs-fast
              xmtp-sdk-generated-node
              xmtp-sdk-generated-browser
              xmtp-sdk-generated-kotlin
              # The browser platform proofs copy these test fixtures.
              xmtp-sdk-wasm-fixture-panic
              xmtp-sdk-pure-fixture-codec
              xmtp-backend
              backend-image-x86_64-unknown-linux-musl
              ;
          }
        );
        # The arm64 Node pairs are cross outputs of this system; release
        # substitutes them (build-sdk-node-platforms.yml).
        baseRelease = mkSet pkgs "base-release" {
          inherit (p)
            xmtp-sdk-node-linux-x64-gnu
            xmtp-sdk-node-linux-x64-musl
            xmtp-sdk-node-linux-arm64-gnu
            xmtp-sdk-node-linux-arm64-musl
            android-sdk-libs
            # release-android.yml (`just example-android apk`).
            example-android-apk
            ;
        };
        # For the JavaScript suites and docs: Node/browser trees, lightweight
        # RN JS/types, and browser fixtures. Mobile components stay separate.
        sdk_js = mkSet pkgs "sdk_js" {
          inherit (p)
            xmtp-sdk-generated-node
            xmtp-sdk-generated-browser
            xmtp-sdk-react-native-js
            xmtp-sdk-wasm-fixture-panic
            xmtp-sdk-pure-fixture-codec
            ;
        };
        # For the Android suites: the Kotlin tree, the JNI libraries (one ABI
        # and all four), and the JNA host library.
        sdk_android = mkSet pkgs "sdk_android" {
          inherit (p)
            xmtp-sdk-generated-kotlin
            android-sdk-libs-fast
            android-sdk-libs
            xmtp-sdk-libs
            ;
        };
      };
    aarch64-darwin =
      { pkgs, self', ... }:
      let
        p = self'.packages;
      in
      rec {
        dev = mkSet pkgs "dev" (baseDev.passthru.members // rnDev pkgs p);
        release = mkSet pkgs "release" (baseRelease.passthru.members // rnRelease pkgs p);
        baseDev = mkSet pkgs "base-dev" (
          shells self'
          // {
            inherit (p)
              android-sdk-libs-fast
              ios-xcframeworks-fast
              xmtp-sdk-generated-node
              xmtp-sdk-generated-browser
              services-test
              backend-image-aarch64-unknown-linux-musl
              ;
          }
        );
        baseRelease = mkSet pkgs "base-release" {
          inherit (p)
            ios-xcframeworks
            xmtp-sdk-node-darwin-arm64
            ;
        };
      };
  };
in
{
  flake.ciSets = lib.mapAttrs (
    system: set:
    withSystem system (
      args:
      builtins.removeAttrs (set args) [
        "baseDev"
        "baseRelease"
      ]
    )
  ) sets;
  flake.ciStages = lib.mapAttrs (
    system: set:
    withSystem system (
      args:
      let
        products = set args;
        allMembers = products.dev.passthru.members // products.release.passthru.members;
        rnMembers = lib.filterAttrs (
          name: _:
          lib.hasPrefix "xmtp-sdk-react-native-" name
          || name == "react-native-js-tools"
          || name == "react-native-js-tools-deps"
          || name == "ubjs-react-native"
          || name == "xmtp-sdk-generated-react-native"
        ) allMembers;
      in
      {
        base = mkSet args.pkgs "base" (
          products.baseDev.passthru.members // products.baseRelease.passthru.members
        );
        native-base = mkSet args.pkgs "rn-native-base" (
          {
            inherit (args.self'.packages) xmtp-sdk-libs xmtp-sdk-bindgen;
          }
          // (
            if system == "aarch64-darwin" then
              {
                inherit (args.self'.packages) xmtp-sdk-ios-device xmtp-sdk-ios-simulator;
              }
            else
              lib.listToAttrs (
                map (target: {
                  name = "xmtp-sdk-android-${target.abi}";
                  value = args.self'.packages.${"xmtp-sdk-android-${target.abi}"};
                }) args.pkgs.xmtp.sdkPlatforms.android
              )
          )
        );
        components = mkSet args.pkgs "rn-components" (
          builtins.removeAttrs rnMembers [ "xmtp-sdk-react-native-package" ]
        );
        consumers = mkSet args.pkgs "rn-consumers" (
          lib.filterAttrs (name: _: lib.hasPrefix "react-native-consumer-" name) allMembers
        );
        package = mkSet args.pkgs "rn-package" (
          lib.optionalAttrs (system == "x86_64-linux") {
            inherit (allMembers) xmtp-sdk-react-native-package;
          }
        );
      }
    )
  ) sets;
}
