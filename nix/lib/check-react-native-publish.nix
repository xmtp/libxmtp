{
  stdenvNoCC,
  writeShellScriptBin,
  python311,
  sdkBuildPhase,
}:
let
  stagingFixture = writeShellScriptBin "node" ''
    mkdir -p "$TMPDIR/staged/cpp/generated"
    for name in xmtp_sdk.cpp xmtp_sdk_ffi.hpp xmtp_sdk.hpp; do
      printf 'generated fixture: %s\n' "$name" > "$TMPDIR/staged/cpp/generated/$name"
    done
    chmod -R a-w "$TMPDIR/staged/cpp/generated"
    python3 - "$TMPDIR/staged" "$out" <<'PY'
    import errno
    import os
    import sys

    try:
        os.rename(sys.argv[1], sys.argv[2])
    except OSError as error:
        if error.errno != errno.EXDEV:
            raise
    else:
        raise SystemExit("publication regression requires a cross-filesystem sandbox")
    PY
  '';
in
stdenvNoCC.mkDerivation {
  name = "react-native-read-only-publish-check";
  nativeBuildInputs = [
    stagingFixture
    python311
  ];
  phases = [
    "buildPhase"
    "installCheckPhase"
  ];
  buildPhase = builtins.unsafeDiscardStringContext sdkBuildPhase;
  doInstallCheck = true;
  installCheckPhase = ''
    diff -r "$TMPDIR/staged" "$out"
    test -z "$(find "$TMPDIR/staged/cpp/generated" -perm /222 -print)"
  '';
}
