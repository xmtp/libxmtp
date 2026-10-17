# RN products keep Rust compilation, rendering, package glue, and consumers in
# distinct closures. Only Linux assembles the cross-platform npm product.
{ self, ... }:
{
  perSystem =
    {
      pkgs,
      lib,
      system,
      ...
    }:
    let
      p = self.packages.${system};
      sdk = ../sdks/react-native;
      sdkProvenance = (pkgs.callPackage ./lib/sdk-sources.nix { }).provenance;
      # Package code and native adapters do not enter the Rust SDK source.
      jsSource = lib.fileset.toSource {
        root = ./..;
        fileset = lib.fileset.unions (
          [
            ../Cargo.toml
            ../LICENSE
            ../crates/xmtp_sdk/dev/check-generated-assets.mjs
          ]
          ++ (map (name: lib.fileset.maybeMissing (sdk + "/${name}")) [
            "scripts/stage-package.mjs"
            "src"
            "package.json"
            "tsconfig.json"
            "android"
            "ios"
            "cpp"
            "react-native.config.cjs"
            "XmtpReactNative.podspec"
            "README.md"
          ])
        );
      };
      packageTools = lib.fileset.toSource {
        root = ./..;
        fileset = lib.fileset.unions [
          ../Cargo.toml
          ../LICENSE
          ./sdk-platforms.json
          ../crates/xmtp_sdk/dev/sdk-artifacts.py
          (sdk + /scripts/package.py)
        ];
      };
      posixCommand = pkgs.callPackage ./package/react-native-command.nix { };
      expoCocoapods = pkgs.callPackage ./package/react-native-expo-cocoapods.nix { };
      iosDependencies = pkgs.callPackage ./package/react-native-ios-downloads.nix {
        inherit posixCommand expoCocoapods;
      };
      consumerDeps = lib.genAttrs [ "bare" "expo" ] (
        consumer: pkgs.callPackage ./package/react-native-consumer-deps.nix { inherit consumer; }
      );
      jsTools = pkgs.callPackage ./package/react-native-js-tools.nix { };
      js = pkgs.stdenvNoCC.mkDerivation {
        pname = "xmtp-sdk-react-native-js";
        version = (lib.importJSON (sdk + "/package.json")).version;
        src = jsSource;
        nativeBuildInputs = [
          pkgs.nodejs_26
          jsTools
        ];
        XMTP_SDK_EXPECTED_SOURCE = sdkProvenance.source;
        XMTP_SDK_EXPECTED_RENDER = sdkProvenance.render.react-native;
        XMTP_SDK_TSC = "${jsTools}/bin/tsc";
        XMTP_SDK_REACT_TYPES = "${jsTools}/node_modules/@types/react";
        XMTP_SDK_REACT_NATIVE_TYPES = "${jsTools}/node_modules/react-native";
        buildPhase = ''
          export HOME="$TMPDIR/home"
          mkdir -p "$HOME"
          node sdks/react-native/scripts/stage-package.mjs \
            --generated ${p.xmtp-sdk-generated-react-native} \
            --runtime ${p.ubjs-react-native} --out "$TMPDIR/staged"
          mv "$TMPDIR/staged" "$out"
        '';
        installPhase = "true";
        dontFixup = true;
      };
      androidEntries = map (target: {
        name = target.abi;
        target = target.rustTarget;
        path = p.${"xmtp-sdk-android-${target.abi}"};
      }) pkgs.xmtp.sdkPlatforms.android;
      iosEntries = [
        {
          name = "aarch64-apple-ios";
          target = "aarch64-apple-ios";
          path = p.xmtp-sdk-ios-device;
        }
        {
          name = "aarch64-apple-ios-sim";
          target = "aarch64-apple-ios-sim";
          path = p.xmtp-sdk-ios-simulator;
        }
      ];
      component =
        platform: entries: fast:
        let
          libraries = pkgs.linkFarm "xmtp-sdk-react-native-${platform}-inputs" (
            map (entry: { inherit (entry) name path; }) entries
          );
          toolchains = pkgs.writeText "rn-${platform}-native-toolchains.json" (
            builtins.toJSON (
              lib.listToAttrs (
                map (entry: {
                  name = entry.target;
                  value = (import ./lib/sdk-native-toolchain.nix) entry.path.compilation;
                }) entries
              )
            )
          );
        in
        pkgs.runCommand "xmtp-sdk-react-native-${platform}${lib.optionalString fast "-fast"}"
          {
            nativeBuildInputs = [ pkgs.python311 ];
          }
          ''
            python3 ${packageTools}/sdks/react-native/scripts/package.py component ${platform} \
              --js ${js} --libraries ${libraries} --out "$TMPDIR/component" \
              --toolchain-identity ${toolchains} ${lib.optionalString fast "--fast"}
            cp -R "$TMPDIR/component" "$out"
          '';
      mkConsumers =
        { package }:
        let
          platform = if system == "aarch64-darwin" then "ios" else "android";
          packageVersion = package.passthru.packageVersion;
          expectedSource = sdkProvenance.source;
          consumers = lib.genAttrs [ "bare" "expo" ] (
            consumer:
            pkgs.callPackage ./package/react-native-consumer.nix {
              inherit
                consumer
                platform
                package
                jsTools
                posixCommand
                expoCocoapods
                packageVersion
                expectedSource
                ;
              dependencies = consumerDeps.${consumer};
              nativeDependencies = if platform == "ios" then iosDependencies else null;
              rnShell = self.devShells.${system}.react-native;
            }
          );
        in
        (pkgs.linkFarm "react-native-consumers-${platform}-${packageVersion}" consumers).overrideAttrs {
          passthru = {
            members = consumers;
            inherit package;
          };
        };
      mkPackage =
        {
          version ? (lib.importJSON (sdk + "/package.json")).version,
        }:
        pkgs.runCommand "xmtp-sdk-react-native-package-${version}"
          {
            nativeBuildInputs = [
              pkgs.python311
              pkgs.nodejs_26
            ];
            passthru = {
              packageVersion = version;
              components = {
                android = self.packages.x86_64-linux.xmtp-sdk-react-native-android;
                ios = self.packages.aarch64-darwin.xmtp-sdk-react-native-ios;
              };
            };
          }
          ''
            export HOME="$TMPDIR/home"
            mkdir -p "$HOME"
            # These are immutable foreign-system bytes. Producers must finish
            # exporting/importing them before this Linux derivation is built.
            test -f ${self.packages.aarch64-darwin.xmtp-sdk-react-native-ios}/sdk-contract.json
            python3 ${packageTools}/sdks/react-native/scripts/package.py assemble \
              --js ${self.packages.x86_64-linux.xmtp-sdk-react-native-js} \
              --android ${self.packages.x86_64-linux.xmtp-sdk-react-native-android} \
              --ios ${self.packages.aarch64-darwin.xmtp-sdk-react-native-ios} \
              --version ${lib.escapeShellArg version} --out "$TMPDIR/package"
            cp -R "$TMPDIR/package" "$out"
          '';
    in
    {
      legacyPackages.reactNative = { inherit mkPackage mkConsumers; };
      packages = {
        xmtp-sdk-react-native-js = js;
        react-native-js-tools = jsTools;
        react-native-js-tools-deps = jsTools.dependencies;
        react-native-consumer-deps-bare = consumerDeps.bare;
        react-native-consumer-deps-expo = consumerDeps.expo;
      }
      // lib.optionalAttrs (system == "x86_64-linux") {
        xmtp-sdk-react-native-android = component "android" androidEntries false;
        xmtp-sdk-react-native-android-fast = component "android" (builtins.filter (
          entry: entry.name == "x86_64"
        ) androidEntries) true;
        xmtp-sdk-react-native-package = mkPackage { };
        react-native-consumer-android = mkConsumers { package = p.xmtp-sdk-react-native-package; };
        react-native-consumer-native-deps-android = pkgs.linkFarm "rn-android-native-dependencies" (
          (lib.mapAttrs (
            _: drv: drv.passthru.nativeDependencies
          ) p.react-native-consumer-android.passthru.members)
          // {
            gradle-toolchains = self.devShells.${system}.react-native.passthru.gradle;
          }
        );
      }
      // lib.optionalAttrs (system == "aarch64-darwin") {
        react-native-consumer-native-deps-ios = iosDependencies;
        react-native-consumer-ios = mkConsumers {
          package = self.packages.x86_64-linux.xmtp-sdk-react-native-package;
        };
        xmtp-sdk-react-native-ios = component "ios" iosEntries false;
        xmtp-sdk-react-native-ios-simulator = component "ios" (builtins.filter (
          entry: entry.target == "aarch64-apple-ios-sim"
        ) iosEntries) true;
      };
    };
}
