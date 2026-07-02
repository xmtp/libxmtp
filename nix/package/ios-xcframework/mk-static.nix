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
        srcLib = "${dylibs.iphone64}/libxmtpv3.a";
      }
    )
    ++ lib.optional (simAbis != [ ]) (
      helpers.mkSlice {
        platform = "ios";
        abis = simAbis;
        variant = "simulator";
      }
      // {
        srcLib = "$TMPDIR/lipo_sim/libxmtpv3.a";
      }
    )
    ++ lib.optional (macAbis != [ ]) (
      helpers.mkSlice {
        platform = "macos";
        abis = macAbis;
      }
      // {
        srcLib = "$TMPDIR/lipo_macos/libxmtpv3.a";
      }
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

    # Assembled by hand: an xcframework is one directory per slice plus a
    # manifest. `xcodebuild -create-xcframework` does the same job but
    # dlopens host Xcode first-launch frameworks, which can't be sandboxed.
    XCF=$out/LibXMTPSwiftFFI.xcframework
    ${lib.concatMapStrings (s: ''
      mkdir -p $XCF/${s.id}/Headers
      cp ${s.srcLib} $XCF/${s.id}/libxmtpv3.a
      cp -r ${headerDir}/. $XCF/${s.id}/Headers/
    '') slices}
    cp ${infoPlist} $XCF/Info.plist

    echo "Validating static xcframework..."
    test -f $XCF/Info.plist || { echo "FAIL: Missing xcframework Info.plist"; exit 1; }
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
    [ "$FOUND" -ge ${toString expectedSlices} ] || \
      { echo "FAIL: Expected >= ${toString expectedSlices} static slices, found $FOUND"; exit 1; }
    echo "Static xcframework validation passed ($FOUND slices)"
  '';
}
