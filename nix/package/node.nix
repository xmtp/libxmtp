{
  runCommand,
  lib,
  stdenv,
  buildPackages,
  xmtpNative,
  ubrnNative,
  runtimeRevision,
  jq,
  darwin,
  patchelf,
}:
let
  target = stdenv.hostPlatform.rust.rustcTarget;
  napiTarget =
    {
      aarch64-apple-darwin = "darwin-arm64";
      x86_64-apple-darwin = "darwin-x64";
      aarch64-unknown-linux-gnu = "linux-arm64-gnu";
      x86_64-unknown-linux-gnu = "linux-x64-gnu";
      aarch64-unknown-linux-musl = "linux-arm64-musl";
      x86_64-unknown-linux-musl = "linux-x64-musl";
      x86_64-pc-windows-msvc = "win32-x64-msvc";
    }
    .${target};
  addon = "uniffi-runtime-napi.${napiTarget}.node";
  library = if stdenv.hostPlatform.isDarwin then "libxmtp_sdk.dylib" else "libxmtp_sdk.so";
in
runCommand "xmtp-sdk-node-${napiTarget}"
  {
    nativeBuildInputs = [
      jq
    ]
    ++ lib.optionals stdenv.hostPlatform.isMusl [ patchelf ]
    ++ lib.optionals stdenv.hostPlatform.isDarwin [
      darwin.autoSignDarwinBinariesHook
      buildPackages.darwin.cctools
    ];
  }
  (
    ''
      test "$(jq -r .revision ${ubrnNative}/runtime-provenance.json)" = '${runtimeRevision}'
      test "$(jq -r .target ${ubrnNative}/runtime-provenance.json)" = '${target}'
      test "$(jq -r .addon ${ubrnNative}/runtime-provenance.json)" = '${addon}'
      test "$(jq -r .schema ${ubrnNative}/runtime-provenance.json)" = 1
      mkdir -p "$out/lib" "$out/runtime"
      cp ${xmtpNative}/lib/${library} "$out/lib/${library}"
      cp ${xmtpNative}/native-provenance.json "$out/native-provenance.json"
      cp ${ubrnNative}/${addon} "$out/runtime/${addon}"
      cp ${ubrnNative}/runtime-provenance.json "$out/runtime/runtime-provenance.json"
      chmod u+w "$out/lib/${library}" "$out/runtime/${addon}"
    ''
    + lib.optionalString stdenv.hostPlatform.isMusl ''
      patchelf --remove-rpath "$out/lib/${library}"
      patchelf --remove-rpath "$out/runtime/${addon}"
    ''
    + lib.optionalString stdenv.hostPlatform.isDarwin ''
      for binary in "$out/lib/${library}" "$out/runtime/${addon}"; do
        if otool -l "$binary" | awk '/cmd LC_ID_DYLIB/ { found=1 } END { exit !found }'; then
          install_name_tool -id "@loader_path/$(basename "$binary")" "$binary"
        fi
        otool -L "$binary" \
          | awk 'NR > 1 && $1 ~ /^\/nix\/store\/.*\/libiconv(\.[0-9]+)*\.dylib$/ { print $1 }' \
          | while read -r old; do
            install_name_tool -change "$old" "/usr/lib/$(basename "$old")" "$binary"
          done
        otool -l "$binary" \
          | awk '/cmd LC_RPATH/ { rpath=1; next } rpath && $1 == "path" { if ($2 ~ /^\/nix\/store\//) print $2; rpath=0 }' \
          | while read -r old; do
            install_name_tool -delete_rpath "$old" "$binary"
          done
        sign "$binary"
        remaining=$(otool -L "$binary" | awk 'NR > 1 && $1 ~ /^\/nix\/store\// { print $1 }')
        rpaths=$(otool -l "$binary" | awk '/cmd LC_RPATH/ { rpath=1; next } rpath && $1 == "path" { if ($2 ~ /^\/nix\/store\//) print $2; rpath=0 }')
        if [ -n "$remaining$rpaths" ]; then
          echo "error: $binary retains a Nix load path or rpath" >&2
          exit 1
        fi
      done
    ''
  )
