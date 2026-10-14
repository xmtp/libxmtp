{ stdenvNoCC }:
{
  compilation,
  provenance,
  target,
}:
# The receipt names the SDK closure identity that Nix computed at evaluation
# (`sdk-sources.nix`). It takes no source, runs no Python, and names no store
# path, so a commit outside the closure keeps this path.
stdenvNoCC.mkDerivation {
  inherit (compilation) pname version;
  dontUnpack = true;
  # The compiler output has already passed fixup. Keep its exact bytes.
  dontFixup = true;
  receipt = builtins.toJSON {
    schema = 1;
    inherit (provenance) source;
    inherit target;
    profile = "release";
    features = "";
  };
  passAsFile = [ "receipt" ];
  installPhase = ''
    mkdir -p "$out/lib"
    cp -R ${compilation}/lib/. "$out/lib/"
    cp "$receiptPath" "$out/native-provenance.json"
  '';
  passthru = {
    inherit compilation provenance;
  };
}
