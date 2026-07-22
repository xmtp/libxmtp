{
  stdenv,
}:
{
  static,
  dynamic,
  swiftBindings,
  licenseFile,
  version,
}:
stdenv.mkDerivation {
  pname = "xmtpv3-ios-xcframeworks";
  inherit version;
  dontUnpack = true;
  dontFixup = true;
  installPhase = ''
    mkdir -p $out/LibXMTPSwiftFFI/Sources/LibXMTP
    cp -r ${static}/LibXMTPSwiftFFI.xcframework $out/LibXMTPSwiftFFI/
    cp ${swiftBindings}/swift/xmtpv3.swift $out/LibXMTPSwiftFFI/Sources/LibXMTP/
    cp ${licenseFile} $out/LibXMTPSwiftFFI/LICENSE

    mkdir -p $out/LibXMTPSwiftFFIDynamic/Sources/LibXMTP
    cp -r ${dynamic}/LibXMTPSwiftFFIDynamic.xcframework $out/LibXMTPSwiftFFIDynamic/
    cp ${swiftBindings}/swift/xmtpv3.swift $out/LibXMTPSwiftFFIDynamic/Sources/LibXMTP/
    cp ${licenseFile} $out/LibXMTPSwiftFFIDynamic/LICENSE
  '';
}
