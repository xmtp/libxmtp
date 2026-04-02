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
      # Single source of truth: ABI name → target config
      iosTargets = {
        "aarch64-darwin" = {
          config = "arm64-apple-darwin";
        };
        "x86_64-darwin" = {
          config = "x86_64-apple-darwin";
        };
        "iphone64" = {
          config = "arm64-apple-ios";
          darwinSdkVersion = "26.3";
          darwinMinVersion = "16";
          xcodeVer = "26.3";
        };
        "iphone64-simulator" = {
          config = "aarch64-apple-ios-simulator";
          darwinSdkVersion = "26";
          darwinMinVersion = "16";
          xcodeVer = "26.3";
        };
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

      # Swift bindings from host build
      inherit (mkIosBindings pkgs { }) swift-bindings;

      fastAbi =
        if pkgs.stdenv.hostPlatform.isx86_64 then
          "x86_64-darwin"
        else if pkgs.stdenv.hostPlatform.isAarch64 then
          "aarch64-darwin"
        else
          throw "Unsupported host architecture for ios-libs-fast";

      fastDylib = iosDylibs.${fastAbi};

      ios-libs-fast = pkgs.linkFarm "xmtpv3-ios-fast" [
        {
          name = "jniLibs/${fastAbi}/libuniffi_xmtpv3.so";
          path = "${fastDylib}/libuniffi_xmtpv3.so";
        }
        {
          name = "java/uniffi/xmtpv3/xmtpv3.kt";
          path = "${swift-bindings}/kotlin/uniffi/xmtpv3/xmtpv3.kt";
        }
        {
          name = "libxmtp-version.txt";
          path = "${swift-bindings}/libxmtp-version.txt";
        }
      ];

      # Aggregate all targets + Swift bindings into a Gradle-ready layout
      ios-libs = pkgs.linkFarm "xmtpv3-ios" (
        lib.mapAttrsToList (abi: dylib: {
          name = "jniLibs/${abi}/libuniffi_xmtpv3.so";
          path = "${dylib}/libuniffi_xmtpv3.so";
        }) iosDylibs
        ++ [
          {
            name = "java/uniffi/xmtpv3/xmtpv3.kt";
            path = "${swift-bindings}/kotlin/uniffi/xmtpv3/xmtpv3.kt";
          }
          {
            name = "libxmtp-version.txt";
            path = "${swift-bindings}/libxmtp-version.txt";
          }
        ]
      );
    in
    {
      packages = {
        inherit ios-libs ios-libs-fast swift-bindings;

      }
      // lib.mapAttrs' (abi: dylib: {
        name = "ios-bindings-${abi}";
        value = dylib;
      }) iosDylibs;
    };
}
