# iOS xcframework assembly — static, dynamic, release, and dev outputs.
#
# Hermetic: the xcframework bundle is assembled by hand (one directory per
# slice plus an Info.plist manifest) instead of via
# `xcodebuild -create-xcframework`, which dlopens host Xcode first-launch
# frameworks and so can't run inside the build sandbox. Only cctools
# (lipo/install_name_tool/otool) and rcodesign are needed — no Xcode.
# See docs/specs/2026-05-06-ios-xcframework-redesign.md.
{
  lib,
  pkgs,
  callPackage,
}:
let
  helpers = callPackage ./helpers.nix { };
  # Inject helpers into the callPackage scope so each mk-*.nix can request
  # it by name without explicit `inherit` plumbing at every call.
  callPackage' = lib.callPackageWith (pkgs // { inherit helpers; });
in
{
  mkStatic = callPackage' ./mk-static.nix { };
  mkDynamic = callPackage' ./mk-dynamic.nix { };
  mkRelease = callPackage' ./mk-release.nix { };
  mkDev = callPackage' ./mk-dev.nix { };
}
