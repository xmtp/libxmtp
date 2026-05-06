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
        darwinMinVersion = "16";
        xcodeVer = "26.3";
        # Wrap builder-host Xcode (iosSdkPkgs) instead of building the Darwin
        # toolchain from source (~94 drvs vs ~2500). Comment out to fall back
        # to the hermetic apple-sdk source path.
        useiOSPrebuilt = true;
      };
      # Single source of truth: ABI name → target config
      iosTargets = {
        "aarch64-darwin" = {
          config = "arm64-apple-darwin";
          xcodeVer = "26.3";
        };
        "x86_64-darwin" = {
          config = "x86_64-apple-darwin";
          xcodeVer = "26.3";
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

      inherit (mkIosBindings pkgs { }) swift-bindings version;

      fastAbi =
        if pkgs.stdenv.hostPlatform.isx86_64 then
          "x86_64-darwin"
        else if pkgs.stdenv.hostPlatform.isAarch64 then
          "aarch64-darwin"
        else
          throw "Unsupported host architecture for ios-libs-fast";

      # xcframework derivations are host-side packaging tools — call them through
      # the local cross-pkgs so darwin.xcode auto-resolves via crossSystem.xcodeVer.
      hostPkgs = crossPkgs.${fastAbi};
      xcode-tools = hostPkgs.callPackage ./lib/packages/xcode-tools.nix { };
      xcframework = hostPkgs.callPackage ./package/ios-xcframework { inherit xcode-tools; };

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
        inherit version;
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

      ios-libs-fast = mkLibs "xmtpv3-ios-fast" [ fastAbi ];
      ios-libs = mkLibs "xmtpv3-ios" allAbis;
    in
    {
      packages = {
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
      }) iosDylibs;
    };
}
