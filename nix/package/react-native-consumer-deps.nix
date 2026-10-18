# Lock-only dependency sources. App, SDK, adapter, and documentation edits do
# not change this layer. Hashes were measured with pnpm's reproducible v4 store.
{
  lib,
  fetchPnpmDeps,
  xmtp-pnpm,
  consumer,
}:
let
  fixture = ../../sdks/react-native/test/consumers + "/${consumer}";
  hashes = {
    bare = "sha256-6e+V5SjTFo2xKjjxrTEeyLikpCqCOtteNWl/rE43lMk=";
    expo = "sha256-OLw8Ubti/Sbr8U5t85kQ7jzKr//4ejaqx4oXES9rupg=";
  };
in
fetchPnpmDeps {
  pname = "react-native-${consumer}";
  pnpm = xmtp-pnpm;
  fetcherVersion = 4;
  hash = hashes.${consumer};
  src = lib.fileset.toSource {
    root = fixture;
    fileset = lib.fileset.unions (
      map (name: fixture + "/${name}") [
        "package.json"
        "pnpm-lock.yaml"
        "pnpm-workspace.yaml"
      ]
    );
  };
}
