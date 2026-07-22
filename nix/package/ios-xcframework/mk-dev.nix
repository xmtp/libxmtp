{
  lib,
  stdenv,
}:
{
  static,
  dynamic ? null,
  swiftBindings,
  version,
}:
stdenv.mkDerivation {
  pname = "xmtpv3-ios-xcframeworks-dev";
  inherit version;
  dontUnpack = true;
  dontFixup = true;
  installPhase = ''
    mkdir -p $out
    cp -r ${static}/LibXMTPSwiftFFI.xcframework $out/
    ${lib.optionalString (dynamic != null) ''
      cp -r ${dynamic}/LibXMTPSwiftFFIDynamic.xcframework $out/
    ''}
    cp ${swiftBindings}/swift/xmtpv3.swift $out/xmtpv3.swift
  '';
}
