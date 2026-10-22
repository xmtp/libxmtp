# RN products keep Rust compilation, rendering, package glue, and consumers in
# distinct closures. Only Linux assembles the cross-platform npm product.
#
# Pull requests test a partial package instead: `xmtp-sdk-react-native-test-package`
# holds only the fast components (x86_64 Android, iOS simulator), its version
# ends with `-partial`, and `package.py verify-package` rejects it. The Android
# smoke uses `xmtp-sdk-react-native-test-package-android`, the same package
# without iOS, so it needs no macOS build. Main, the post-merge smoke, and
# releases keep the full package.
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
            ../crates/xmtp_sdk/dev/package-staging.mjs
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
      consumerTemplates = lib.genAttrs [ "bare" "expo" ] (
        consumer: pkgs.callPackage ./package/react-native-consumer-template.nix { inherit consumer; }
      );
      jsTools = pkgs.callPackage ./package/react-native-js-tools.nix { };
      js = pkgs.stdenvNoCC.mkDerivation {
        pname = "xmtp-sdk-react-native-js";
        version = (lib.importJSON (sdk + "/package.json")).version;
        src = jsSource;
        nativeBuildInputs = [
          pkgs.xmtp.nodejs_26
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
          cp -R "$TMPDIR/staged" "$out"
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
        {
          package,
          names ? [
            "bare"
            "expo"
          ],
        }:
        let
          platform = if system == "aarch64-darwin" then "ios" else "android";
          packageVersion = package.passthru.packageVersion;
          expectedSource = sdkProvenance.source;
          consumers = lib.genAttrs names (
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
              template = consumerTemplates.${consumer};
              dependencies = consumerDeps.${consumer};
              nativeDependencies = if platform == "ios" then iosDependencies else null;
              rnShell = self.devShells.${system}.react-native;
            }
          );
        in
        (pkgs.linkFarm "react-native-consumers-${platform}-${packageVersion}${
          lib.optionalString (
            names != [
              "bare"
              "expo"
            ]
          ) "-${lib.concatStringsSep "-" names}"
        }" consumers).overrideAttrs
          {
            passthru = {
              members = consumers;
              inherit package;
            };
          };
      # partial: the CI test package of the fast components. Its version gets
      # package.py's PARTIAL_SUFFIX, which no release version carries.
      # ios = false leaves out the iOS component; only a partial package may.
      mkPackage =
        {
          version ? (lib.importJSON (sdk + "/package.json")).version,
          partial ? false,
          ios ? true,
        }:
        assert lib.assertMsg (ios || partial) "only a partial RN package may leave out iOS";
        let
          packageVersion = version + lib.optionalString partial "-partial";
          components =
            if partial then
              {
                android = self.packages.x86_64-linux.xmtp-sdk-react-native-android-fast;
              }
              // lib.optionalAttrs ios {
                ios = self.packages.aarch64-darwin.xmtp-sdk-react-native-ios-simulator;
              }
            else
              {
                android = self.packages.x86_64-linux.xmtp-sdk-react-native-android;
                ios = self.packages.aarch64-darwin.xmtp-sdk-react-native-ios;
              };
        in
        pkgs.runCommand
          "xmtp-sdk-react-native-package-${packageVersion}${lib.optionalString (!ios) "-android"}"
          {
            nativeBuildInputs = [
              pkgs.python311
              pkgs.xmtp.nodejs_26
            ];
            passthru = {
              inherit packageVersion components;
            };
          }
          ''
            export HOME="$TMPDIR/home"
            mkdir -p "$HOME"
            ${lib.optionalString ios ''
              # These are immutable foreign-system bytes. Producers must finish
              # exporting/importing them before this Linux derivation is built.
              test -f ${components.ios}/sdk-contract.json
            ''}
            python3 ${packageTools}/sdks/react-native/scripts/package.py assemble \
              --js ${self.packages.x86_64-linux.xmtp-sdk-react-native-js} \
              --android ${components.android} \
              ${lib.optionalString ios "--ios ${components.ios}"} \
              --version ${lib.escapeShellArg packageVersion} ${lib.optionalString partial "--partial "}--out "$TMPDIR/package"
            cp -R "$TMPDIR/package" "$out"
          '';
      # The producer stages of the partial test package, in the shape of
      # `ciStages.<system>.<stage>`: a link farm whose passthru.members are the
      # roots that nix/react-native-exchange.py exports and imports.
      stage =
        name: members:
        (pkgs.linkFarm "rn-test-${name}" members).overrideAttrs {
          passthru = { inherit members; };
        };
      testStages = {
        components = stage "components" (
          if system == "x86_64-linux" then
            { inherit (p) xmtp-sdk-react-native-js xmtp-sdk-react-native-android-fast; }
          else if system == "aarch64-darwin" then
            { inherit (p) xmtp-sdk-react-native-ios-simulator; }
          else
            { }
        );
        package = stage "package" (
          lib.optionalAttrs (system == "x86_64-linux") {
            inherit (p) xmtp-sdk-react-native-test-package;
          }
        );
      };
      # The stages of the Android-only test package (the Android smoke). No
      # stage has a Darwin member.
      androidTestStages = {
        components = stage "android-components" (
          lib.optionalAttrs (system == "x86_64-linux") {
            inherit (p) xmtp-sdk-react-native-js xmtp-sdk-react-native-android-fast;
          }
        );
        package = stage "android-package" (
          lib.optionalAttrs (system == "x86_64-linux") {
            inherit (p) xmtp-sdk-react-native-test-package-android;
          }
        );
      };
    in
    {
      legacyPackages.reactNative = {
        inherit
          mkPackage
          mkConsumers
          testStages
          androidTestStages
          ;
      };
      packages = {
        xmtp-sdk-react-native-js = js;
        react-native-js-tools = jsTools;
        react-native-js-tools-deps = jsTools.dependencies;
        react-native-consumer-deps-bare = consumerDeps.bare;
        react-native-consumer-deps-expo = consumerDeps.expo;
        react-native-consumer-template-bare = consumerTemplates.bare;
        react-native-consumer-template-expo = consumerTemplates.expo;
      }
      // lib.optionalAttrs (system == "x86_64-linux") {
        xmtp-sdk-react-native-android = component "android" androidEntries false;
        xmtp-sdk-react-native-android-fast = component "android" (builtins.filter (
          entry: entry.name == "x86_64"
        ) androidEntries) true;
        xmtp-sdk-react-native-package = mkPackage { };
        xmtp-sdk-react-native-test-package = mkPackage { partial = true; };
        xmtp-sdk-react-native-test-package-android = mkPackage {
          partial = true;
          ios = false;
        };
        react-native-consumer-android = mkConsumers { package = p.xmtp-sdk-react-native-package; };
        react-native-test-consumer-android = mkConsumers {
          package = p.xmtp-sdk-react-native-test-package;
        };
        # The Android smoke: the bare app of the Android-only test package.
        react-native-test-consumer-android-bare = mkConsumers {
          package = p.xmtp-sdk-react-native-test-package-android;
          names = [ "bare" ];
        };
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
        react-native-test-consumer-ios = mkConsumers {
          package = self.packages.x86_64-linux.xmtp-sdk-react-native-test-package;
        };
        xmtp-sdk-react-native-ios = component "ios" iosEntries false;
        xmtp-sdk-react-native-ios-simulator = component "ios" (builtins.filter (
          entry: entry.target == "aarch64-apple-ios-sim"
        ) iosEntries) true;
      };
    };
}
