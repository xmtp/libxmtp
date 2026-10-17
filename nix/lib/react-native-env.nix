# RN/Expo adapters and consumers use a separate pin from sibling Rust slices.
# Hosted availability: Warp's macOS 26 image lists Xcode 27.0 (27A266a).
{ lib }:
let
  xcode = rec {
    version = "27.0";
    build = "27A266a";
    developerDir = "/Applications/Xcode_27.0.app/Contents/Developer";
    key = "Xcode ${version} (${build}) at ${developerDir}";
  };
  checkXcode = ''
    export DEVELOPER_DIR=${lib.escapeShellArg xcode.developerDir}
    found=$("$DEVELOPER_DIR/usr/bin/xcodebuild" -version)
    expected=$'Xcode ${xcode.version}\nBuild version ${xcode.build}'
    if [[ "$found" != "$expected" ]]; then
      echo "error: RN requires ${xcode.key}; found $found" >&2
      exit 1
    fi
    export XMTP_RN_XCODE_MODE=canonical
    export XMTP_RN_XCODE_IDENTITY=${lib.escapeShellArg xcode.key}
    unset SDKROOT
    export PATH="$DEVELOPER_DIR/usr/bin:$DEVELOPER_DIR/Toolchains/XcodeDefault.xctoolchain/usr/bin:/usr/bin:$PATH"
  '';
in
{
  inherit xcode checkXcode;
  localXcode = ''
    export DEVELOPER_DIR=$(/usr/bin/env -u DEVELOPER_DIR /usr/bin/xcode-select -p)
    found=$("$DEVELOPER_DIR/usr/bin/xcodebuild" -version)
    version=$(printf '%s\n' "$found" | /usr/bin/sed -n 's/^Xcode //p')
    build=$(printf '%s\n' "$found" | /usr/bin/sed -n 's/^Build version //p')
    test -n "$version" && test -n "$build"
    export XMTP_RN_XCODE_MODE=local
    export XMTP_RN_XCODE_IDENTITY="Xcode $version ($build) at $DEVELOPER_DIR"
    unset SDKROOT
    export PATH="$DEVELOPER_DIR/usr/bin:$DEVELOPER_DIR/Toolchains/XcodeDefault.xctoolchain/usr/bin:/usr/bin:$PATH"
  '';

}
