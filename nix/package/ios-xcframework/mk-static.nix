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
  pname = "xmtpv3-static-xcframework";
  inherit version;
  dontUnpack = true;
  dontFixup = true;
  nativeBuildInputs = [ xcode-tools ];
  installPhase = ''
    set -euo pipefail
    echo "=== Building static xcframework ==="

    ${helpers.mkLipoSnippet {
      group = "sim";
      ext = "a";
      inherit dylibs;
      abis = simAbis;
    }}
    ${helpers.mkLipoSnippet {
      group = "macos";
      ext = "a";
      inherit dylibs;
      abis = macAbis;
    }}

    XCF_ARGS=""
    ${lib.optionalString (deviceAbis != [ ]) ''
      XCF_ARGS="$XCF_ARGS -library ${dylibs.iphone64}/libxmtpv3.a -headers ${headerDir}"
    ''}
    ${lib.optionalString (simAbis != [ ]) ''
      XCF_ARGS="$XCF_ARGS -library $TMPDIR/lipo_sim/libxmtpv3.a -headers ${headerDir}"
    ''}
    ${lib.optionalString (macAbis != [ ]) ''
      XCF_ARGS="$XCF_ARGS -library $TMPDIR/lipo_macos/libxmtpv3.a -headers ${headerDir}"
    ''}

    mkdir -p $out
    xcodebuild -create-xcframework \
      $XCF_ARGS \
      -output $out/LibXMTPSwiftFFI.xcframework

    echo "Validating static xcframework..."
    FOUND=0
    for slice in $out/LibXMTPSwiftFFI.xcframework/*/; do
      [ -d "$slice/Headers" ] || continue
      FOUND=$((FOUND + 1))
      test -f "$slice/Headers/xmtpv3FFI.h" || { echo "FAIL: Missing xmtpv3FFI.h in $slice"; exit 1; }
      test -f "$slice/Headers/module.modulemap" || { echo "FAIL: Missing modulemap in $slice"; exit 1; }
      head -1 "$slice/Headers/module.modulemap" | grep -q "module xmtpv3FFI" || \
        { echo "FAIL: Bad modulemap in $slice"; exit 1; }
      echo "  static OK: $(basename $slice)"
    done
    [ "$FOUND" -ge ${toString expectedSlices} ] || \
      { echo "FAIL: Expected >= ${toString expectedSlices} static slices, found $FOUND"; exit 1; }
    echo "Static xcframework validation passed ($FOUND slices)"
  '';
}
