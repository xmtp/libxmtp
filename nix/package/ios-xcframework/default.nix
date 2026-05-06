# iOS xcframework assembly — static, dynamic, release, and dev outputs.
#
# Hermetic: no __noChroot. mk-static and mk-dynamic depend on xcode-tools
# (Xcode + cctools + rcodesign); xcode-tools is passed in via callPackage from
# ios-packages.nix using a cross-pkgs whose system config sets xcodeVer.
# See docs/specs/2026-05-06-ios-xcframework-redesign.md.
{
  lib,
  pkgs,
  callPackage,
  xcode-tools,
}:
let
  helpers = callPackage ./helpers.nix { };
  # Inject xcode-tools and helpers into the callPackage scope so each mk-*.nix
  # can request them by name without explicit `inherit` plumbing at every call.
  callPackage' = lib.callPackageWith (pkgs // { inherit xcode-tools helpers; });
in
{
  mkStatic = callPackage' ./mk-static.nix { };
  mkDynamic = callPackage' ./mk-dynamic.nix { };
  mkRelease = callPackage' ./mk-release.nix { };
  mkDev = callPackage' ./mk-dev.nix { };
}
