#!/bin/bash
set -e
cd "$(dirname "$0")"
cd ..

# This runs on `pod install` to arrange the apple dependencies.
# For builds (macOS/iOS/iOS simulator) it puts the XCFramework in the proper location.
# TODO: For tests (macOS) it puts the shared libxmtp_bindings_flutter.dylib in the proper location.
# See `bindings_flutter/lib/src/loader.dart`
#
# Note: flutter/dart doesn't let us publish large assets with our package (limit ~100mb total).
#       So this script fetch the assets from a github published release instead.
#
#       For subsequent runs (or for local development) this will use already downloaded assets
#       from the `artifacts/apple/` folder when they exist.
#
#       Once flutter native assets are better supported we may want to migrate.
#       See https://github.com/flutter/flutter/issues/129757 (native assets)

FrameworkName=${1:-"xmtp_bindings_flutter"}
FrameworkArchiveFile=${2:-"xmtp_bindings_flutter.xcframework.zip"}
FrameworkArchiveUrl=${3:-"https://example.com/xmtp/libxmtp/releases/download/foo-tag/version"}

if [ -f "artifacts/apple/${FrameworkArchiveFile}" ]
then
 echo
 echo "Using available $FrameworkName"
 echo "  artifacts/apple/${FrameworkArchiveFile}"
 echo
 cp "artifacts/apple/${FrameworkArchiveFile}" "macos/Frameworks/${FrameworkArchiveFile}"
 cp "artifacts/apple/${FrameworkArchiveFile}" "ios/Frameworks/${FrameworkArchiveFile}"
else
 echo
 echo "Downloading $FrameworkName"
 echo "  $FrameworkArchiveUrl"
 echo
 curl -f -L -o "artifacts/apple/${FrameworkArchiveFile}" "$FrameworkArchiveUrl"
 cp "artifacts/apple/${FrameworkArchiveFile}" "macos/Frameworks/${FrameworkArchiveFile}"
 cp "artifacts/apple/${FrameworkArchiveFile}" "ios/Frameworks/${FrameworkArchiveFile}"
fi

for platform in "macos" "ios"
do
  echo "Unzipping $FrameworkName for $platform"
  cd "./$platform/Frameworks/"
  rm -rf $FrameworkName
  unzip -q $FrameworkArchiveFile
  cd ../../
done
