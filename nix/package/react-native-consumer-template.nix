# Native scaffold source only. Locks, test code, and package changes do not
# invalidate the cached reconstruction of the exact published templates.
{
  lib,
  runCommand,
  fetchurl,
  python311,
  patch,
  consumer,
}:
let
  fixtures = ../../sdks/react-native/test/consumers;
  fixture = fixtures + "/${consumer}";
  pin = lib.importJSON (fixture + /template-source.json);
  archive = fetchurl {
    inherit (pin) url;
    hash = pin.integrity;
  };
in
runCommand "react-native-consumer-template-${consumer}-${pin.version}"
  {
    nativeBuildInputs = [
      python311
      patch
    ];
  }
  ''
    python3.11 ${fixtures + /prepare-template.py} \
      --consumer ${consumer} --archive ${archive} \
      --patch ${fixture + /native.patch} --output "$out"
  ''
