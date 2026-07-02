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
  inherit (helpers.classifyTargets abis)
    deviceAbis
    simAbis
    macAbis
    expectedSlices
    ;
  headerDir = "${swiftBindings}/swift/include/libxmtp";

  slices =
    lib.optional (deviceAbis != [ ]) (
      helpers.mkSlice {
        platform = "ios";
        abis = deviceAbis;
      }
      // {
        srcFw = "$TMPDIR/fw_ios/xmtpv3FFI.framework";
      }
    )
    ++ lib.optional (simAbis != [ ]) (
      helpers.mkSlice {
        platform = "ios";
        abis = simAbis;
        variant = "simulator";
      }
      // {
        srcFw = "$TMPDIR/fw_sim/xmtpv3FFI.framework";
      }
    )
    ++ lib.optional (macAbis != [ ]) (
      helpers.mkSlice {
        platform = "macos";
        abis = macAbis;
      }
      // {
        srcFw = "$TMPDIR/fw_mac/xmtpv3FFI.framework";
      }
    );

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

    ${lib.optionalString (deviceAbis != [ ]) (
      helpers.mkFrameworkBundle {
        name = "fw_ios";
        dylibPath = "${dylibs.iphone64}/libxmtpv3.dylib";
        minOSVersion = iosMinVersion;
        inherit headerDir;
      }
    )}
    ${lib.optionalString (simAbis != [ ]) (
      helpers.mkFrameworkBundle {
        name = "fw_sim";
        dylibPath = "$TMPDIR/lipo_sim/libxmtpv3.dylib";
        minOSVersion = iosMinVersion;
        inherit headerDir;
      }
    )}
    ${lib.optionalString (macAbis != [ ]) (
      helpers.mkFrameworkBundle {
        name = "fw_mac";
        dylibPath = "$TMPDIR/lipo_macos/libxmtpv3.dylib";
        minOSVersion = macMinVersion;
        isMacOS = true;
        inherit headerDir;
      }
    )}

    # Assembled by hand: an xcframework is one directory per slice plus a
    # manifest. `xcodebuild -create-xcframework` does the same job but
    # dlopens host Xcode first-launch frameworks, which can't be sandboxed.
    # cp -R (not -rL) so the macOS bundle's Versions/ symlinks survive.
    XCF=$out/LibXMTPSwiftFFIDynamic.xcframework
    ${lib.concatMapStrings (s: ''
      mkdir -p $XCF/${s.id}
      cp -R ${s.srcFw} $XCF/${s.id}/
    '') slices}
    cp ${infoPlist} $XCF/Info.plist

    echo "Validating dynamic xcframework..."
    test -f $XCF/Info.plist || { echo "FAIL: Missing xcframework Info.plist"; exit 1; }
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
    [ "$FOUND" -ge ${toString expectedSlices} ] || \
      { echo "FAIL: Expected >= ${toString expectedSlices} dynamic slices, found $FOUND"; exit 1; }
    echo "Dynamic xcframework validation passed ($FOUND slices)"
  '';
}
