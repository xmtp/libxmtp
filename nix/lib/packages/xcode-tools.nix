# Apple toolchain bundle (Xcode + cctools + rcodesign) for hermetic
# xcframework assembly — everything `xcodebuild -create-xcframework` and the
# .framework bundling in nix/package/ios-xcframework need on one PATH.
#
# Must be called through a cross-pkgs whose system config sets xcodeVer
# (ios-packages.nix pins 26.3); otherwise darwin.xcode falls back to the
# nixpkgs default xcode_12_3 and downstream builds break.
{
  lib,
  runCommand,
  symlinkJoin,
  cctools,
  rcodesign,
  darwin,
}:
let
  # Xcode binaries live under Contents/Developer/usr/bin/, not bin/ — lift them
  # into a flat bin/ so the standard PATH hook picks them up.
  xcode-bins = runCommand "xcode-bins" { } ''
    mkdir -p $out/bin
    ln -s ${darwin.xcode}/Contents/Developer/usr/bin/* $out/bin/
  '';
in
symlinkJoin {
  name = "xcode-tools";
  paths = [
    cctools
    rcodesign
    xcode-bins
  ];
  meta = {
    description = "Apple toolchain (Xcode + cctools + rcodesign) for hermetic xcframework builds";
    platforms = lib.platforms.darwin;
  };
}
