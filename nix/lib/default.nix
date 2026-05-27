{
  inputs,
  self,
  lib,
  ...
}:
let
  patches = import ./patches.nix;

  # napi + uniffi builders live in the crane scope
  craneConfig = final: prev: {
    napiBuild = final.callPackage ./napiBuild.nix { };
    uniffiGenerate = final.callPackage ./uniffiGenerate.nix { };
  };

  # `host` is the HOST pkgset (kept stable across cross pkgsets so build
  # tools stay host-built). `final` is the current pkgset — use it for
  # anything that must follow the target (toolchain, stdenv, crane).
  xmtpOverlay = host: final: _: rec {
    ffi-uniffi-bindgen = host.callPackage ./packages/uniffi-bindgen.nix { };
    wasm-bindgen-cli = host.callPackage ./packages/wasm-bindgen-cli.nix { };
    napi-rs-cli = host.callPackage ./packages/napi-rs-cli { };
    swiftlint = host.callPackage ./packages/swiftlint.nix { };
    xmtp = {
      inherit ffi-uniffi-bindgen;
      filesets = host.callPackage ./filesets.nix { };
      mkToolchain = final.callPackage ./mkToolchain.nix { inherit inputs; };
      mkNativeToolchain = xmtp.mkToolchain final;
      xdbg-driver-lib = final.callPackage ./../package/xdbg-driver-lib { };
      craneLib = (inputs.crane.mkLib final).overrideScope craneConfig;
      base = final.callPackage ./base.nix { };
      androidEnv = final.callPackage ./android-env.nix { };
      iosEnv = final.callPackage ./ios-env.nix { };
      shellCommon = host.callPackage ./shell-common.nix { };
      mkVersion = import ./mkVersion.nix;
      toNapiTarget = import ./napiTarget.nix;
      gitSha = self.shortRev or self.dirtyShortRev or "unknown";
      gitCommitDate = self.lastModifiedDate or "";
      cross-version-test = final.callPackage ./../package/cross-version-test { };
      cross-talk-test = final.callPackage ./../package/cross-talk-test { };
    };
  };

  baseOverlays = [
    inputs.fenix.overlays.default
    inputs.foundry.overlay
  ]
  ++ patches;

  config = {
    android_sdk.accept_license = true;
    allowUnfree = true;
  };

  mkHostPkgs =
    system:
    let
      hostPkgs = import inputs.nixpkgs {
        inherit system config;
        overlays = baseOverlays ++ [ (xmtpOverlay hostPkgs) ];
      };
    in
    hostPkgs;

  # crossSystem=null returns the host pkgset directly (no double import).
  mkXmtpPkgs =
    {
      system,
      crossSystem ? null,
    }:
    let
      hostPkgs = mkHostPkgs system;
    in
    if crossSystem == null then
      hostPkgs
    else
      import inputs.nixpkgs {
        inherit config crossSystem;
        localSystem = system;
        overlays = baseOverlays ++ [ (xmtpOverlay hostPkgs) ];
      };

  normalize =
    x:
    if builtins.isString x then
      { config = x; }
    else if builtins.isAttrs x then
      x
    else
      throw "expected a string or attribute set";
in
{
  flake.lib = {
    inherit mkXmtpPkgs;

    # Build cross pkgsets sharing one host pkgset across all targets.
    mkCrossPkgs =
      system: targets:
      let
        hostPkgs = mkHostPkgs system;
        overlays = baseOverlays ++ [ (xmtpOverlay hostPkgs) ];
      in
      lib.listToAttrs (
        map (
          target:
          let
            t = normalize target;
          in
          {
            name = t.config;
            value = import inputs.nixpkgs {
              inherit config overlays;
              localSystem = system;
              crossSystem = t;
            };
          }
        ) targets
      );
  };
}
