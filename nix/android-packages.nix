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

      # Per-target dylibs keyed by config name
      sdkDylibs = lib.mapAttrs (
        _: p: p.callPackage ./package/xmtp-sdk-native.nix { android = true; }
      ) crossPkgs;
      generatedKotlin = "${self.packages.${system}.xmtp-sdk-generated-kotlin}/kotlin";
      sdkSources =
        map
          (name: {
            inherit name;
            path = "${generatedKotlin}/${name}";
          })
          [
            "uniffi"
            "runtime"
            "android"
            "sdk-contract.json"
          ];
      sdkLibraries =
        targets:
        lib.mapAttrsToList (config: dylib: {
          name = "jniLibs/${configToAbi.${config}}/libxmtp_sdk.so";
          path = "${dylib}/lib/libxmtp_sdk.so";
        }) targets;

      fastAbi =
        if pkgs.stdenv.hostPlatform.isx86_64 then
          "x86_64"
        else if pkgs.stdenv.hostPlatform.isAarch64 then
          "arm64-v8a"
        else
          throw "Unsupported host architecture for android-sdk-libs-fast";

      fastTarget = androidTargets.${fastAbi};

      android-sdk-libs-fast = pkgs.linkFarm "xmtp-sdk-android-fast" (
        sdkSources ++ sdkLibraries { ${fastTarget.config} = sdkDylibs.${fastTarget.config}; }
      );
      android-sdk-libs = pkgs.linkFarm "xmtp-sdk-android" (sdkSources ++ sdkLibraries sdkDylibs);

    in
    {
      packages = {
        inherit
          android-sdk-libs
          android-sdk-libs-fast
          ;
      }
      // lib.mapAttrs' (config: crossPkgs: {
        name = "xmtp-sdk-android-${configToAbi.${config}}";
        value = sdkDylibs.${config};
      }) crossPkgs;
    };
}
