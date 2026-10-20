# Warm sets: the outputs that .github/workflows/post-merge.yml builds, pushes to the
# trusted Cachix cache `xmtp`, and pins on every push to trunk.
#
# `ciSets.<system>.dev` holds what developers and CI jobs use;
# `ciSets.<system>.release` holds what release jobs use. The post-merge
# `build` job builds and pins both. `ciSets.<system>.rn` holds the RN outputs
# and RN dev shells; the post-merge RN cache jobs build and pin it after
# `build`. The RN device chain is advisory, so an RN break can reach trunk.
# dev and release hold only the RN outputs that blocking pull request suites
# build (`rnBlocking`, Linux only), so such a break holds back only the RN
# jobs, not the other sets or the docs. Each set is a link farm, so one store
# path holds the runtime closure of every member, and `passthru.members`
# lists the members for the closure check of post-merge.yml.
#
# A dev shell enters as its `inputDerivation`, whose runtime closure holds
# every shell input. Add a member only when a job, a release, or a developer
# builds that output from trunk.
#
# Every other set is a product group of .github/ci-suites.yml: CI builds
# `ciSets.x86_64-linux.<group>` once per run in the job `products_<group>`.
# post-merge.yml's `build` builds only dev and release, and an RN break can
# stop rn, so keep each group member in dev or release. Its `closure` job
# checks every set but rn, and `rn-alert` checks rn. dev/ci/rn-isolation
# (in nix-eval.yml) fails when a set other than rn builds an rn member.
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

  # The RN JavaScript layer: the SDK's JavaScript product and its build
  # inputs.
  rnJs = p: {
    inherit (p)
      react-native-js-tools
      react-native-js-tools-deps
      ubjs-react-native
      xmtp-sdk-generated-react-native
      xmtp-sdk-react-native-js
      ;
  };

  isRnShell = name: lib.hasPrefix "shell-react-native" name;

  # Every RN output and RN dev shell of the system.
  rnAll =
    pkgs: self':
    let
      p = self'.packages;
    in
    rnJs p
    // lib.filterAttrs (name: _: isRnShell name) (shells self')
    // {
      inherit (p)
        react-native-consumer-deps-bare
        react-native-consumer-deps-expo
        react-native-consumer-template-bare
        react-native-consumer-template-expo
        ;
    }
    // lib.optionalAttrs pkgs.stdenv.isLinux {
      inherit (p)
        xmtp-sdk-react-native-android-fast
        xmtp-sdk-react-native-android
        xmtp-sdk-react-native-package
        react-native-consumer-android
        react-native-consumer-native-deps-android
        ;
    }
    // lib.optionalAttrs pkgs.stdenv.isDarwin {
      inherit (p)
        xmtp-sdk-react-native-ios-simulator
        xmtp-sdk-react-native-ios
        react-native-consumer-ios
        react-native-consumer-native-deps-ios
        ;
    };

  # The RN outputs that blocking pull request suites build, so an RN break
  # cannot reach them. Only Linux has them: the JavaScript layer, which the
  # JavaScript suites and the docs read (`sdk_js`), and the `react-native`
  # shell that `check_react_native` enters. No blocking suite builds RN on
  # Darwin.
  rnBlocking =
    self':
    rnJs self'.packages
    // {
      inherit (shells self') shell-react-native;
    };

  # The dev shells but the RN ones.
  nonRnShells = self': lib.filterAttrs (name: _: !isRnShell name) (shells self');

  sets = {
    x86_64-linux =
      { pkgs, self', ... }:
      let
        p = self'.packages;
      in
      {
        rn = mkSet pkgs "rn" (
          builtins.removeAttrs (rnAll pkgs self') (builtins.attrNames (rnBlocking self'))
        );
        dev = mkSet pkgs "dev" (
          nonRnShells self'
          // rnBlocking self'
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
              xmtp-legacy-migration-fixture-host
              xmtp-backend
              backend-image-x86_64-unknown-linux-musl
              ;
          }
        );
        # The arm64 Node pairs are cross outputs of this system; release
        # substitutes them (build-sdk-node-platforms.yml).
        release = mkSet pkgs "release" {
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
            xmtp-legacy-migration-fixture-host
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
      {
        rn = mkSet pkgs "rn" (rnAll pkgs self');
        dev = mkSet pkgs "dev" (
          nonRnShells self'
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
        release = mkSet pkgs "release" {
          inherit (p)
            ios-xcframeworks
            xmtp-sdk-node-darwin-arm64
            ;
        };
      };
  };
in
{
  flake.ciSets = lib.mapAttrs (system: withSystem system) sets;
  flake.ciStages = lib.mapAttrs (
    system: set:
    withSystem system (
      args:
      let
        products = set args;
        allMembers =
          products.dev.passthru.members // products.release.passthru.members // products.rn.passthru.members;
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
