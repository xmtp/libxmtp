# Shared bb library for cross-version-test and cross-talk-test.
# Stages src/ + bb.edn under $out/lib/src for downstream classpath use.
{ runCommand }:
runCommand "xdbg-driver-lib" { } ''
  install -dm755 $out/lib
  cp -r --no-preserve=mode ${./src} $out/lib/src
  install -Dm644 ${./bb.edn} $out/lib/src/bb.edn
''
