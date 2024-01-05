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
#       We can ship them for now because they fit.
#       But eventually we'll need this script to fetch the assets from a github published release instead.
#       See also https://github.com/flutter/flutter/issues/129757 (native assets)
#
# This script looks if the file already exists and otherwise goes and fetches it.
# Then it unzips it into the macos/ and ios/ frameworks so it gets included with binary builds.
#

FrameworkName=${1:-"xmtp_bindings_flutter"}
FrameworkArchiveFile=${2:-"xmtp_bindings_flutter.xcframework.zip"}
FrameworkArchiveUrl=${3:-"https://example.com/xmtp/libxmtp/releases/download/foo-tag/version"}

# TODO: instead fetch this from a published release
#  curl -L $FrameworkArchiveUrl -o $FrameworkArchiveFile
if [ -f "artifacts/apple/${FrameworkArchiveFile}" ]
then
 cp -v "artifacts/apple/${FrameworkArchiveFile}" "macos/Frameworks/${FrameworkArchiveFile}"
 cp -v "artifacts/apple/${FrameworkArchiveFile}" "ios/Frameworks/${FrameworkArchiveFile}"
fi

pushd macos/Frameworks/
rm -rf $FrameworkName
unzip $FrameworkArchiveFile
popd

pushd ios/Frameworks/
rm -rf $FrameworkName
unzip $FrameworkArchiveFile
popd
