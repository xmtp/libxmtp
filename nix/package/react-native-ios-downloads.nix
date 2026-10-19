# Exact RN/Hermes archives observed by the locked CocoaPods resolution. The
# consumer's curl replay checks these hashes and rejects every undeclared URL.
{
  lib,
  fetchurl,
  linkFarm,
  rsync,
  perl,
  posixCommand,
  expoCocoapods,
}:
let
  manifest = lib.importJSON ../../sdks/react-native/test/consumers/ios-downloads.json;
in
linkFarm "react-native-ios-native-dependencies" (
  map (
    artifact:
    assert lib.assertMsg (
      !(lib.hasPrefix "/" artifact.path) && !(lib.elem ".." (lib.splitString "/" artifact.path))
    ) "RN native download must stay within its declared input tree";
    {
      name = artifact.path;
      path = fetchurl { inherit (artifact) url sha256; };
    }
  ) manifest.artifacts
  ++ [
    {
      name = "tools/rsync";
      path = rsync;
    }
    {
      name = "tools/perl";
      path = perl;
    }
    {
      name = "tools/command";
      path = posixCommand;
    }
    {
      name = "tools/expo-cocoapods";
      path = expoCocoapods;
    }
  ]
)
