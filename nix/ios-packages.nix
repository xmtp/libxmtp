{ self, ... }:
{
  perSystem =
    {
      pkgs,
      system,
      lib,
      ...
    }:
    let
      iosCommon = {
        darwinSdkVersion = "26";
        # Deployment floor, not SDK version — asserted below to match the
        # floors advertised in Package.swift and the podspec.
        darwinMinVersion = "14";
        xcodeVer = "26.3";
        # Wrap builder-host Xcode (iosSdkPkgs) instead of building the Darwin
        # toolchain from source (~94 drvs vs ~2500). Comment out to fall back
        # to the hermetic apple-sdk source path.
        useiOSPrebuilt = true;
      };
      # macOS deployment floor for the mac slices, same contract as
      # iosCommon.darwinMinVersion.
      macMinVersion = "11.0";
      # Single source of truth: ABI name → target config
      iosTargets = {
        "aarch64-darwin" = {
          config = "arm64-apple-darwin";
          xcodeVer = "26.3";
          darwinMinVersion = macMinVersion;
        };
        "x86_64-darwin" = {
          config = "x86_64-apple-darwin";
          xcodeVer = "26.3";
          darwinMinVersion = macMinVersion;
        };
        "iphone64" = {
          config = "arm64-apple-ios";
        }
        // iosCommon;
        "iphone64-simulator" = {
          config = "aarch64-apple-ios-simulator";
        }
        // iosCommon;
        # "aarch64-ios-macabi" = {
        #   config = "arm64-apple-darwin";
        #   rust.rustcTarget = "aarch64-apple-ios-macabi";
        # };
        # "x86_64-ios-macabi" = {
        #   config = "x86_64-apple-darwin";
        #   rust.rustcTarget = "x86_64-apple-ios-macabi";
        # };
      };

      # crossPkgs keyed by ABI name (via the `name` field passed to mkCrossPkgs)
      crossPkgs = self.lib.mkCrossPkgs system (
        lib.mapAttrsToList (abi: t: t // { name = abi; }) iosTargets
      );
      mkIosBindings = p: p.callPackage ./package/ios-bindings.nix;

      # Per-target dylibs keyed by ABI name
      iosDylibs = lib.mapAttrs (_: p: (mkIosBindings p { }).dylib) crossPkgs;

      # The native build only feeds uniffi bindings generation — it never
      # ships, so skip the from-source static openssl.
      inherit (mkIosBindings pkgs { staticOpenssl = false; }) swift-bindings version;

      # The floors advertised to SPM/CocoaPods must match what the binaries
      # are built with (LC_BUILD_VERSION minos); catch drift at eval time.
      iosMinVersion =
        assert lib.assertMsg (lib.hasInfix ".iOS(.v${iosCommon.darwinMinVersion})" (
          builtins.readFile "${self}/Package.swift"
        )) "Package.swift .iOS floor doesn't match darwinMinVersion ${iosCommon.darwinMinVersion}";
        assert lib.assertMsg (lib.hasInfix "deployment_target = '${iosCommon.darwinMinVersion}.0'" (
          builtins.readFile "${self}/sdks/ios/XMTP.podspec"
        )) "XMTP.podspec deployment_target doesn't match darwinMinVersion ${iosCommon.darwinMinVersion}";
        assert lib.assertMsg (lib.hasInfix ".macOS(.v${lib.versions.major macMinVersion})" (
          builtins.readFile "${self}/Package.swift"
        )) "Package.swift .macOS floor doesn't match macMinVersion ${macMinVersion}";
        iosCommon.darwinMinVersion + ".0";

      fastAbi =
        if pkgs.stdenv.hostPlatform.isx86_64 then
          "x86_64-darwin"
        else if pkgs.stdenv.hostPlatform.isAarch64 then
          "aarch64-darwin"
        else
          throw "Unsupported host architecture for ios-libs-fast";

      # xcframework derivations are host-side packaging (lipo/plist/sign) —
      # call them through NATIVE pkgs so their stdenv stays substitutable.
      xcframework = pkgs.callPackage ./package/ios-xcframework { };

      allAbis = lib.attrNames iosDylibs;
      fastAbis = [
        fastAbi
        "iphone64-simulator"
      ];

      ios-xcframework-static = xcframework.mkStatic {
        abis = allAbis;
        dylibs = iosDylibs;
        swiftBindings = swift-bindings;
        inherit version;
      };
      ios-xcframework-dynamic = xcframework.mkDynamic {
        abis = allAbis;
        dylibs = iosDylibs;
        swiftBindings = swift-bindings;
        inherit iosMinVersion macMinVersion version;
      };
      ios-xcframework-static-fast = xcframework.mkStatic {
        abis = fastAbis;
        dylibs = iosDylibs;
        swiftBindings = swift-bindings;
        inherit version;
      };

      ios-release = xcframework.mkRelease {
        static = ios-xcframework-static;
        dynamic = ios-xcframework-dynamic;
        swiftBindings = swift-bindings;
        licenseFile = ../LICENSE;
        inherit version;
      };
      ios-devFast = xcframework.mkDev {
        static = ios-xcframework-static-fast;
        dynamic = null;
        swiftBindings = swift-bindings;
        inherit version;
      };

      # Build an xcframework-ready linkFarm from the given ABIs.
      mkLibs =
        name: abis:
        pkgs.linkFarm name (
          lib.concatMap (abi: [
            {
              name = "${abi}/libxmtpv3.a";
              path = "${iosDylibs.${abi}}/libxmtpv3.a";
            }
            {
              name = "${abi}/libxmtpv3.dylib";
              path = "${iosDylibs.${abi}}/libxmtpv3.dylib";
            }
          ]) abis
          ++ [
            {
              name = "swift/xmtpv3.swift";
              path = "${swift-bindings}/swift/xmtpv3.swift";
            }
            {
              name = "swift/include";
              path = "${swift-bindings}/swift/include";
            }
            {
              name = "libxmtp-version.txt";
              path = "${swift-bindings}/libxmtp-version.txt";
            }
          ]
        );

      ios-libs-fast = mkLibs "xmtpv3-ios-fast" fastAbis;
      ios-libs = mkLibs "xmtpv3-ios" allAbis;
    in
    {
      # iOS packaging needs darwin.xcode — only exposed on Darwin hosts.
      # Linux workstations build via .#packages.aarch64-darwin.<attr> on a
      # remote Darwin builder.
      packages = lib.optionalAttrs pkgs.stdenv.isDarwin (
        {
          inherit
            ios-libs
            ios-libs-fast
            swift-bindings
            ios-xcframework-static
            ios-xcframework-dynamic
            ios-release
            ios-devFast
            ;
        }
        // lib.mapAttrs' (abi: dylib: {
          name = "ios-bindings-${abi}";
          value = dylib;
        }) iosDylibs
      );
    };
}
