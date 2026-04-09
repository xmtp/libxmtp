#!/usr/bin/env bash
# test-xcframeworks.sh <output-dir> [expected-slices]
# Validates xcframework structure for both static and dynamic variants.
set -euo pipefail
OUT="$1"
EXPECTED="${2:-3}"

echo "=== Static xcframework ==="
XCF="$OUT/LibXMTPSwiftFFI/LibXMTPSwiftFFI.xcframework"
test -d "$XCF" || XCF="$OUT/LibXMTPSwiftFFI.xcframework"
test -d "$XCF" || { echo "FAIL: No static xcframework found at $OUT"; exit 1; }
STATIC_FOUND=0
for slice in "$XCF"/*/; do
    [ -d "$slice/Headers" ] || continue
    STATIC_FOUND=$((STATIC_FOUND + 1))
    test -f "$slice/Headers/xmtpv3FFI.h" || { echo "FAIL: Missing xmtpv3FFI.h in $slice"; exit 1; }
    test -f "$slice/Headers/module.modulemap" || { echo "FAIL: Missing modulemap in $slice"; exit 1; }
    head -1 "$slice/Headers/module.modulemap" | grep -q "module xmtpv3FFI" || \
        { echo "FAIL: Bad modulemap content in $slice"; exit 1; }
    echo "  static OK: $(basename $slice)"
done
[ "$STATIC_FOUND" -ge "$EXPECTED" ] || { echo "FAIL: Expected >= $EXPECTED static slices, found $STATIC_FOUND"; exit 1; }

echo "=== Dynamic xcframework ==="
DXCF="$OUT/LibXMTPSwiftFFIDynamic/LibXMTPSwiftFFIDynamic.xcframework"
if [ ! -d "$DXCF" ]; then
    echo "SKIP: No dynamic xcframework (expected for fast builds)"
else
    DYN_FOUND=0
    for fw in "$DXCF"/*/xmtpv3FFI.framework; do
        test -d "$fw" || continue
        DYN_FOUND=$((DYN_FOUND + 1))
        test -f "$fw/xmtpv3FFI" || { echo "FAIL: Missing binary in $fw"; exit 1; }
        test -f "$fw/Info.plist" || { echo "FAIL: Missing Info.plist in $fw"; exit 1; }
        test -f "$fw/Headers/xmtpv3FFI.h" || { echo "FAIL: Missing header in $fw"; exit 1; }
        test -f "$fw/Modules/module.modulemap" || { echo "FAIL: Missing modulemap in $fw"; exit 1; }
        head -1 "$fw/Modules/module.modulemap" | grep -q "^framework module xmtpv3FFI" || \
            { echo "FAIL: modulemap missing 'framework module' prefix in $fw"; exit 1; }
        INSTALL_NAME=$(otool -D "$fw/xmtpv3FFI" | tail -1)
        echo "$INSTALL_NAME" | grep -q "@rpath/xmtpv3FFI.framework/xmtpv3FFI" || \
            { echo "FAIL: Bad install name '$INSTALL_NAME' in $fw"; exit 1; }
        echo "  dynamic OK: $(basename $(dirname $fw))"
    done
    [ "$DYN_FOUND" -ge "$EXPECTED" ] || { echo "FAIL: Expected >= $EXPECTED dynamic slices, found $DYN_FOUND"; exit 1; }
fi

echo "Xcframework validation passed"
