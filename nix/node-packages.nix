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
      # `nix/sdk-platforms.json` names the systems that build each Node
      # platform. glibc cross builds fail on macOS, Apple SDKs exist only on
      # macOS, and the Windows pair builds outside Nix.
      hostTarget = pkgs.stdenv.hostPlatform.rust.rustcTarget;
      platforms = builtins.filter (
        platform: builtins.elem system platform.nixSystems
      ) pkgs.xmtp.sdkPlatforms.node;
      # The host target uses the host package set, so its library is the one
      # host library of `xmtp-sdk.nix`. Other targets get a cross set.
      crossPkgs = self.lib.mkCrossPkgs system (
        lib.remove hostTarget (map (platform: platform.rustTarget) platforms)
      );
      packageSet = rustTarget: if rustTarget == hostTarget then pkgs else crossPkgs.${rustTarget};
      runtime = pkgs.xmtp.ubrn;
      mkSdkNode =
        p:
        p.callPackage ./package/node.nix {
          xmtpNative = p.callPackage ./package/xmtp-sdk-native.nix { };
          ubrnNative = p.callPackage ./package/xmtp-sdk-node-runtime.nix {
            ubrnSrc = runtime.src;
            runtimeRevision = runtime.rev;
          };
          runtimeRevision = runtime.rev;
        };
    in
    {
      packages = lib.listToAttrs (
        map (platform: {
          name = "xmtp-sdk-node-${platform.target}";
          value = mkSdkNode (packageSet platform.rustTarget);
        }) platforms
      );
    };
}
