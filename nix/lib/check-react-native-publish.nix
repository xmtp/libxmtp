{
  stdenvNoCC,
  writeShellScriptBin,
  python311,
  runCommandCC,
  sdkBuildPhase,
}:
let
  # CI may put TMPDIR and the store on the same filesystem. Force the
  # cross-device rename result, while leaving coreutils' real copy/remove
  # fallback and the fixture's filesystem permissions in place.
  crossDeviceRename = runCommandCC "cross-device-rename" { } ''
    mkdir -p "$out/lib"
    cat > rename.c <<'C'
    #include <errno.h>
    #include <stdio.h>

    int rename(const char *old, const char *new) {
        errno = EXDEV;
        return -1;
    }
    int renameat(int oldfd, const char *old, int newfd, const char *new) {
        errno = EXDEV;
        return -1;
    }
    int renameat2(int oldfd, const char *old, int newfd, const char *new, unsigned int flags) {
        errno = EXDEV;
        return -1;
    }
    C
    $CC -shared -fPIC rename.c -o "$out/lib/rename.so"
  '';
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
        raise SystemExit("publication regression requires EXDEV injection")
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
  buildPhase = ''
    export LD_PRELOAD=${crossDeviceRename}/lib/rename.so
    ${builtins.unsafeDiscardStringContext sdkBuildPhase}
    unset LD_PRELOAD
  '';
  doInstallCheck = true;
  installCheckPhase = ''
    diff -r "$TMPDIR/staged" "$out"
    test -z "$(find "$TMPDIR/staged/cpp/generated" -perm /222 -print)"
  '';
}
