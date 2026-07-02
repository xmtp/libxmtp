{
  lib,
  stdenv,
  cctools,
  rcodesign,
  helpers,
}:
{
  abis,
  dylibs,
  swiftBindings,
  version,
  # Advertised minimum iOS version — thread from iosCommon.darwinMinVersion
  # so the plists never drift from what the binaries are built with.
  iosMinVersion,
  macMinVersion,
}:
let
  inherit (helpers.classifyTargets abis) simAbis macAbis;
  headerDir = "${swiftBindings}/swift/include/libxmtp";

  slices = helpers.mkSlices abis (group: "$TMPDIR/fw_${group}/xmtpv3FFI.framework");

  # Source dylib for each slice's framework bundle: the device slice ships
  # the store artifact directly, sim/mac ship the lipo'd fat binary.
  srcDylib =
    group:
    if group == "device" then
      "${dylibs.iphone64}/libxmtpv3.dylib"
    else
      "$TMPDIR/lipo_${group}/libxmtpv3.dylib";

  infoPlist = helpers.mkXCFrameworkPlist (
    map (
      s:
      s
      // {
        libraryPath = "xmtpv3FFI.framework";
        binaryPath =
          if s.platform == "macos" then
            "xmtpv3FFI.framework/Versions/A/xmtpv3FFI"
          else
            "xmtpv3FFI.framework/xmtpv3FFI";
      }
    ) slices
  );
in
stdenv.mkDerivation {
  pname = "xmtpv3-dynamic-xcframework";
  inherit version;
  dontUnpack = true;
  dontFixup = true;
  nativeBuildInputs = [
    cctools
    rcodesign
  ];
  installPhase = ''
    set -euo pipefail
    echo "=== Building dynamic xcframework ==="

    ${helpers.mkLipoSnippet {
      group = "sim";
      ext = "dylib";
      inherit dylibs;
      abis = simAbis;
    }}
    ${helpers.mkLipoSnippet {
      group = "macos";
      ext = "dylib";
      inherit dylibs;
      abis = macAbis;
    }}

    ${lib.concatMapStrings (
      s:
      helpers.mkFrameworkBundle {
        name = "fw_${s.group}";
        dylibPath = srcDylib s.group;
        minOSVersion = if s.platform == "macos" then macMinVersion else iosMinVersion;
        isMacOS = s.platform == "macos";
        inherit headerDir;
      }
    ) slices}

    # Assembled by hand — see default.nix for why xcodebuild is avoided.
    # cp -R (not -rL) so the macOS bundle's Versions/ symlinks survive.
    XCF=$out/LibXMTPSwiftFFIDynamic.xcframework
    ${lib.concatMapStrings (s: ''
      mkdir -p $XCF/${s.id}
      cp -R ${s.src} $XCF/${s.id}/
    '') slices}
    cp ${infoPlist} $XCF/Info.plist

    echo "Validating dynamic xcframework..."
    ${lib.concatMapStrings (
      s: helpers.checkPlatformSnippet s "$XCF/${s.id}/xmtpv3FFI.framework/xmtpv3FFI"
    ) slices}
    FOUND=0
    for fw in $XCF/*/xmtpv3FFI.framework; do
      test -d "$fw" || continue
      FOUND=$((FOUND + 1))
      # macOS slices use the versioned bundle layout (Versions/A + symlinks,
      # Info.plist under Resources/); iOS slices are flat.
      if [ -d "$fw/Versions" ]; then
        PLIST="$fw/Resources/Info.plist"
        WANT_ID="@rpath/xmtpv3FFI.framework/Versions/A/xmtpv3FFI"
      else
        PLIST="$fw/Info.plist"
        WANT_ID="@rpath/xmtpv3FFI.framework/xmtpv3FFI"
      fi
      test -f "$fw/xmtpv3FFI" || { echo "FAIL: Missing binary in $fw"; exit 1; }
      test -f "$PLIST" || { echo "FAIL: Missing Info.plist in $fw"; exit 1; }
      test -d "$fw/Headers" || { echo "FAIL: Missing Headers in $fw"; exit 1; }
      test -f "$fw/Headers/xmtpv3FFI.h" || { echo "FAIL: Missing xmtpv3FFI.h in $fw"; exit 1; }
      test -f "$fw/Modules/module.modulemap" || { echo "FAIL: Missing modulemap in $fw"; exit 1; }
      head -1 "$fw/Modules/module.modulemap" | grep -q "^framework module xmtpv3FFI" || \
        { echo "FAIL: modulemap missing 'framework module' prefix in $fw"; exit 1; }
      INSTALL_NAME=$(otool -D "$fw/xmtpv3FFI" | tail -1)
      echo "$INSTALL_NAME" | grep -q "$WANT_ID" || \
        { echo "FAIL: Bad install name '$INSTALL_NAME' in $fw (want $WANT_ID)"; exit 1; }
      echo "  dynamic OK: $(basename $(dirname $fw))"
    done
    [ "$FOUND" -ge ${toString (lib.length slices)} ] || \
      { echo "FAIL: Expected >= ${toString (lib.length slices)} dynamic slices, found $FOUND"; exit 1; }
    echo "Dynamic xcframework validation passed ($FOUND slices)"
  '';
}
