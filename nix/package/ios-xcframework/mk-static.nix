{
  lib,
  stdenv,
  cctools,
  helpers,
}:
{
  abis,
  dylibs,
  swiftBindings,
  version,
}:
let
  inherit (helpers.classifyTargets abis) simAbis macAbis;
  headerDir = "${swiftBindings}/swift/include/libxmtp";

  slices = helpers.mkSlices abis (
    group:
    if group == "device" then "${dylibs.iphone64}/libxmtpv3.a" else "$TMPDIR/lipo_${group}/libxmtpv3.a"
  );

  infoPlist = helpers.mkXCFrameworkPlist (
    map (
      s:
      s
      // {
        libraryPath = "libxmtpv3.a";
        binaryPath = "libxmtpv3.a";
        headersPath = "Headers";
      }
    ) slices
  );
in
stdenv.mkDerivation {
  pname = "xmtpv3-static-xcframework";
  inherit version;
  dontUnpack = true;
  dontFixup = true;
  nativeBuildInputs = [ cctools ];
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

    # Assembled by hand — see default.nix for why xcodebuild is avoided.
    XCF=$out/LibXMTPSwiftFFI.xcframework
    ${lib.concatMapStrings (s: ''
      mkdir -p $XCF/${s.id}/Headers
      cp ${s.src} $XCF/${s.id}/libxmtpv3.a
      cp -r ${headerDir}/. $XCF/${s.id}/Headers/
    '') slices}
    cp ${infoPlist} $XCF/Info.plist

    echo "Validating static xcframework..."
    ${lib.concatMapStrings (s: helpers.checkPlatformSnippet s "$XCF/${s.id}/libxmtpv3.a") slices}
    FOUND=0
    for slice in $XCF/*/; do
      [ -d "$slice/Headers" ] || continue
      FOUND=$((FOUND + 1))
      test -f "$slice/libxmtpv3.a" || { echo "FAIL: Missing libxmtpv3.a in $slice"; exit 1; }
      test -f "$slice/Headers/xmtpv3FFI.h" || { echo "FAIL: Missing xmtpv3FFI.h in $slice"; exit 1; }
      test -f "$slice/Headers/module.modulemap" || { echo "FAIL: Missing modulemap in $slice"; exit 1; }
      head -1 "$slice/Headers/module.modulemap" | grep -q "module xmtpv3FFI" || \
        { echo "FAIL: Bad modulemap in $slice"; exit 1; }
      echo "  static OK: $(basename $slice)"
    done
    [ "$FOUND" -ge ${toString (lib.length slices)} ] || \
      { echo "FAIL: Expected >= ${toString (lib.length slices)} static slices, found $FOUND"; exit 1; }
    echo "Static xcframework validation passed ($FOUND slices)"
  '';
}
