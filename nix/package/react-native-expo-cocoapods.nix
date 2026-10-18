{
  writeShellScriptBin,
  python311,
  cocoapods,
}:
writeShellScriptBin "pod" ''
  set -euo pipefail
  ${cocoapods}/bin/pod "$@"
  if [ "$#" -gt 0 ] && [ "$1" = install ]; then
    ${python311}/bin/python3.11 ${./react-native-expo-swiftpm.py}
  fi
''
