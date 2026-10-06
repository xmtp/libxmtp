{ stdenvNoCC, python3 }:
{
  compilation,
  source,
  target,
}:
stdenvNoCC.mkDerivation {
  inherit (compilation) pname version;
  src = source;
  nativeBuildInputs = [ python3 ];
  # The compiler output has already passed fixup. Keep its exact bytes.
  dontFixup = true;
  buildPhase = ''
    mkdir -p "$out/lib"
    cp -R ${compilation}/lib/. "$out/lib/"
    python3 - <<'PYTHON' > "$out/native-provenance.json"
    import importlib.util, json
    spec = importlib.util.spec_from_file_location("artifacts", "crates/xmtp_sdk/dev/sdk-artifacts.py")
    artifacts = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(artifacts)
    print(json.dumps({"schema": 1, "source": artifacts.source_hash(), "generator": artifacts.source_hash(True), "target": "${target}", "profile": "release", "features": ""}))
    PYTHON
  '';
  installPhase = "true";
  passthru = {
    inherit compilation;
    provenanceSource = source;
  };
}
