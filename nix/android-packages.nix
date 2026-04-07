{ self, inputs, ... }:
{
  perSystem =
    {
      pkgs,
      system,
      lib,
      ...
    }:
    let
      platforms = import "${inputs.nixpkgs}/lib/systems/platforms.nix" { inherit lib; };

      sdkConfig = {
        androidSdkVersion = "23";
        androidNdkVersion = "27";
        useAndroidPrebuilt = true;
      };

      # Single source of truth: ABI name → target config
      androidTargets = {
        "arm64-v8a" = {
          config = "aarch64-unknown-linux-android";
          rust.rustcTarget = "aarch64-linux-android";
        };
        "armeabi-v7a" = {
          config = "armv7a-unknown-linux-androideabi";
          rust.rustcTarget = "armv7-linux-androideabi";
        }
        // platforms.armv7a-android;
        "x86_64" = {
          config = "x86_64-unknown-linux-android";
          rust.rustcTarget = "x86_64-linux-android";
        };
        "x86" = {
          config = "i686-unknown-linux-android";
          rust.rustcTarget = "i686-linux-android";
        };
      };

      # config name → ABI name
      configToAbi = lib.listToAttrs (
        lib.mapAttrsToList (abi: t: {
          name = t.config;
          value = abi;
        }) androidTargets
      );

      crossPkgs = self.lib.mkCrossPkgs system (lib.mapAttrsToList (_: t: t // sdkConfig) androidTargets);
      mkAndroidBindings = p: p.callPackage ./package/android.nix;

      # Per-target dylibs keyed by config name
      androidDylibs = lib.mapAttrs (_: p: (mkAndroidBindings p { }).dylib) crossPkgs;

      # Kotlin bindings from host build
      inherit (mkAndroidBindings pkgs { }) kotlin-bindings;

      fastAbi =
        if pkgs.stdenv.hostPlatform.isx86_64 then
          "x86_64"
        else if pkgs.stdenv.hostPlatform.isAarch64 then
          "arm64-v8a"
        else
          throw "Unsupported host architecture for android-libs-fast";

      fastTarget = androidTargets.${fastAbi};
      fastDylib = androidDylibs.${fastTarget.config};

      android-libs-fast = pkgs.linkFarm "xmtpv3-android-fast" [
        {
          name = "jniLibs/${fastAbi}/libuniffi_xmtpv3.so";
          path = "${fastDylib}/libuniffi_xmtpv3.so";
        }
        {
          name = "java/uniffi/xmtpv3/xmtpv3.kt";
          path = "${kotlin-bindings}/kotlin/uniffi/xmtpv3/xmtpv3.kt";
        }
        {
          name = "libxmtp-version.txt";
          path = "${kotlin-bindings}/libxmtp-version.txt";
        }
      ];

      # Aggregate all targets + Kotlin bindings into a Gradle-ready layout
      android-libs = pkgs.linkFarm "xmtpv3-android" (
        lib.mapAttrsToList (config: dylib: {
          name = "jniLibs/${configToAbi.${config}}/libuniffi_xmtpv3.so";
          path = "${dylib}/libuniffi_xmtpv3.so";
        }) androidDylibs
        ++ [
          {
            name = "java/uniffi/xmtpv3/xmtpv3.kt";
            path = "${kotlin-bindings}/kotlin/uniffi/xmtpv3/xmtpv3.kt";
          }
          {
            name = "libxmtp-version.txt";
            path = "${kotlin-bindings}/libxmtp-version.txt";
          }
        ]
      );
    in
    {
      packages = {
        inherit android-libs android-libs-fast kotlin-bindings;
      }
      // lib.mapAttrs' (config: dylib: {
        name = "android-bindings-${configToAbi.${config}}";
        value = dylib;
      }) androidDylibs;
    };
}
