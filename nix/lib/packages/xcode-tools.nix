# Apple toolchain bundle (Xcode + cctools + rcodesign) for hermetic
# xcframework assembly — everything `xcodebuild -create-xcframework` and the
# .framework bundling in nix/package/ios-xcframework need on one PATH.
#
# Callable from native pkgs: the Xcode version is an explicit argument
# (resolving via crossSystem.xcodeVer would force a shadow cross pkgset
# whose stdenv rebuilds the world from source just for packaging tools).
{
  lib,
  runCommand,
  symlinkJoin,
  cctools,
  rcodesign,
  darwin,
  xcodeVer,
}:
let
  xcode = darwin."xcode_${lib.replaceStrings [ "." ] [ "_" ] xcodeVer}";
  # Xcode binaries live under Contents/Developer/usr/bin/, not bin/ — lift them
  # into a flat bin/ so the standard PATH hook picks them up.
  xcode-bins = runCommand "xcode-bins" { } ''
    mkdir -p $out/bin
    ln -s ${xcode}/Contents/Developer/usr/bin/* $out/bin/
  '';
  # Only the cctools we actually invoke — the full package would put raw
  # ar/ld/nm/strip on PATH ahead of the stdenv's wrapped toolchain.
  cctools-bins = runCommand "cctools-bins" { } ''
    mkdir -p $out/bin
    ln -s ${cctools}/bin/lipo ${cctools}/bin/install_name_tool ${cctools}/bin/otool $out/bin/
  '';
in
symlinkJoin {
  name = "xcode-tools";
  paths = [
    cctools-bins
    rcodesign
    xcode-bins
  ];
  # xcodebuild resolves its developer dir via xcode-select/DEVELOPER_DIR,
  # not the binary's location — point it at our Xcode, not the sandbox's.
  postBuild = ''
    mkdir -p $out/nix-support
    echo 'export DEVELOPER_DIR="${xcode}/Contents/Developer"' \
      > $out/nix-support/setup-hook
  '';
  meta = {
    description = "Apple toolchain (Xcode + cctools + rcodesign) for hermetic xcframework builds";
    platforms = lib.platforms.darwin;
  };
}
