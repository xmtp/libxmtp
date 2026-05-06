{
  lib,
  stdenv,
  xcode-tools,
  helpers,
}:
{
  abis,
  dylibs,
  swiftBindings,
  version,
}:
let
  inherit (helpers.classifyTargets abis)
    deviceAbis
    simAbis
    macAbis
    expectedSlices
    ;
  headerDir = "${swiftBindings}/swift/include/libxmtp";
in
stdenv.mkDerivation {
  pname = "xmtpv3-dynamic-xcframework";
  inherit version;
  dontUnpack = true;
  dontFixup = true;
  nativeBuildInputs = [ xcode-tools ];
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
        minOSVersion = "14.0";
        inherit headerDir;
      }
    )}
    ${lib.optionalString (simAbis != [ ]) (
      helpers.mkFrameworkBundle {
        name = "fw_sim";
        dylibPath = "$TMPDIR/lipo_sim/libxmtpv3.dylib";
        minOSVersion = "14.0";
        inherit headerDir;
      }
    )}
    ${lib.optionalString (macAbis != [ ]) (
      helpers.mkFrameworkBundle {
        name = "fw_mac";
        dylibPath = "$TMPDIR/lipo_macos/libxmtpv3.dylib";
        minOSVersion = "11.0";
        inherit headerDir;
      }
    )}

    XCF_ARGS=""
    ${lib.optionalString (deviceAbis != [ ]) ''
      XCF_ARGS="$XCF_ARGS -framework $TMPDIR/fw_ios/xmtpv3FFI.framework"
    ''}
    ${lib.optionalString (simAbis != [ ]) ''
      XCF_ARGS="$XCF_ARGS -framework $TMPDIR/fw_sim/xmtpv3FFI.framework"
    ''}
    ${lib.optionalString (macAbis != [ ]) ''
      XCF_ARGS="$XCF_ARGS -framework $TMPDIR/fw_mac/xmtpv3FFI.framework"
    ''}

    mkdir -p $out
    xcodebuild -create-xcframework \
      $XCF_ARGS \
      -output $out/LibXMTPSwiftFFIDynamic.xcframework

    echo "Validating dynamic xcframework..."
    FOUND=0
    for fw in $out/LibXMTPSwiftFFIDynamic.xcframework/*/xmtpv3FFI.framework; do
      test -d "$fw" || continue
      FOUND=$((FOUND + 1))
      test -f "$fw/xmtpv3FFI" || { echo "FAIL: Missing binary in $fw"; exit 1; }
      test -f "$fw/Info.plist" || { echo "FAIL: Missing Info.plist in $fw"; exit 1; }
      test -d "$fw/Headers" || { echo "FAIL: Missing Headers in $fw"; exit 1; }
      test -f "$fw/Headers/xmtpv3FFI.h" || { echo "FAIL: Missing xmtpv3FFI.h in $fw"; exit 1; }
      test -f "$fw/Modules/module.modulemap" || { echo "FAIL: Missing modulemap in $fw"; exit 1; }
      head -1 "$fw/Modules/module.modulemap" | grep -q "^framework module xmtpv3FFI" || \
        { echo "FAIL: modulemap missing 'framework module' prefix in $fw"; exit 1; }
      INSTALL_NAME=$(otool -D "$fw/xmtpv3FFI" | tail -1)
      echo "$INSTALL_NAME" | grep -q "@rpath/xmtpv3FFI.framework/xmtpv3FFI" || \
        { echo "FAIL: Bad install name '$INSTALL_NAME' in $fw"; exit 1; }
      echo "  dynamic OK: $(basename $(dirname $fw))"
    done
    [ "$FOUND" -ge ${toString expectedSlices} ] || \
      { echo "FAIL: Expected >= ${toString expectedSlices} dynamic slices, found $FOUND"; exit 1; }
    echo "Dynamic xcframework validation passed ($FOUND slices)"
  '';
}
