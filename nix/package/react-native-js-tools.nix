# RN package emit uses the workspace's exact TypeScript version. Its native
# compiler and React declarations are separate fixed-output dependency inputs.
{
  lib,
  stdenvNoCC,
  fetchurl,
  autoPatchelfHook,
  linkFarm,
  stdenv,
}:
let
  compilerTarget =
    {
      aarch64-darwin = {
        name = "darwin-arm64";
        hash = "sha512-gowzar9MwS/aRWp6f3a4KUqzRjAZjOsmGNCM6LcTgXum+dBfgsBVMN+AgvOCCbguXyick6LJhpBszxMebJ8syA==";
      };
      x86_64-linux = {
        name = "linux-x64";
        hash = "sha512-EYdf2cNg7rgCWJnxCdJ+F3V39O8ihb37eHAu1LK8oAFizgTQbPOK7zHHXbPt8rX24COqODXeI3sIf0fCXG7H/A==";
      };
      aarch64-linux = {
        name = "linux-arm64";
        hash = "sha512-Qh4eU4/y3yDjnfjjyPYihMj5/ODIlmt+Bzu17OI+fiSRDW57QmU5SiN63exPRNJPKUzcc1INa1NXdrJ+MqHjUQ==";
      };
    }
    .${stdenv.hostPlatform.system};
  compilerArchive = fetchurl {
    url = "https://registry.npmjs.org/@typescript/typescript-${compilerTarget.name}/-/typescript-${compilerTarget.name}-7.0.2.tgz";
    inherit (compilerTarget) hash;
  };
  reactArchive = fetchurl {
    url = "https://registry.npmjs.org/@types/react/-/react-19.3.0.tgz";
    hash = "sha512-N0rFCuH9YoxG9/m61l9MfpJKfmLOVU0em7ipIz6TRgSSkvReLB9vL85GB+yr8Bs5leqpvg96JSwF4ZS1s4viQg==";
  };
  rnArchive = fetchurl {
    url = "https://registry.npmjs.org/react-native/-/react-native-0.88.0.tgz";
    hash = "sha512-v/wee0nYWcammMYBnSat/e3WY/5iaf9YQOMcBrfKT/wPApPPlHp94hgZovBtwhMuthR01yc/8cGn0FpFTE/C4A==";
  };
  cssArchive = fetchurl {
    url = "https://registry.npmjs.org/csstype/-/csstype-3.2.3.tgz";
    hash = "sha512-z1HGKcYy2xA8AGQfwrn0PAy+PB7X/GSj3UVJW9qKyn43xWa+gl5nXmU4qqLMRzWVLFC8KusUX8T/0kCiOYpAIQ==";
  };
in
stdenvNoCC.mkDerivation {
  pname = "xmtp-react-native-js-tools";
  version = "7.0.2";
  dontUnpack = true;
  nativeBuildInputs = lib.optionals stdenv.isLinux [ autoPatchelfHook ];
  buildInputs = lib.optionals stdenv.isLinux [ stdenv.cc.cc.lib ];
  installPhase = ''
    mkdir -p "$out/lib/typescript" "$out/bin" "$out/node_modules/@types/react" "$out/node_modules/csstype" "$out/node_modules/react-native"
    tar xf ${compilerArchive} -C "$out/lib/typescript" --strip-components=1
    tar xf ${reactArchive} -C "$out/node_modules/@types/react" --strip-components=1
    tar xf ${rnArchive} -C "$out/node_modules/react-native" --strip-components=1
    tar xf ${cssArchive} -C "$out/node_modules/csstype" --strip-components=1
    ln -s ../lib/typescript/lib/tsc "$out/bin/tsc"
  '';
  passthru.dependencies = linkFarm "react-native-js-tools-dependencies" {
    compiler = compilerArchive;
    react = reactArchive;
    react-native = rnArchive;
    csstype = cssArchive;
  };
  # Type declarations and native compiler libraries are inputs, not SDK assets.
  dontStrip = true;
}
