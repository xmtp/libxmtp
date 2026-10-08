# Official release archives. Refresh all hashes when the version changes.
{
  lib,
  stdenvNoCC,
  fetchurl,
  bash,
}:
let
  version = "1.0.0";
  sources = {
    aarch64-darwin = {
      target = "aarch64-apple-darwin";
      hash = "sha256-bRvgB50GiahfoEt/7X6qlPfgglnK+4vTYaXCXEyYxKM=";
    };
    x86_64-darwin = {
      target = "x86_64-apple-darwin";
      hash = "sha256-rgeSsX4cXyQ4s5voiIlsIKrwBr71lZxE+jvFvGbZO1s=";
    };
    aarch64-linux = {
      target = "aarch64-unknown-linux-musl";
      hash = "sha256-iKvYSL59MA1OMLhRDr3EWZDa4/lrZZHjSYIJY5zzRwE=";
    };
    x86_64-linux = {
      target = "x86_64-unknown-linux-musl";
      hash = "sha256-dW6XAaavuDVP2LHXYWThMnLTIDVNhPARl+V9ijtL45c=";
    };
  };
  source = sources.${stdenvNoCC.hostPlatform.system};
in
stdenvNoCC.mkDerivation {
  pname = "kache";
  inherit version;
  src = fetchurl {
    url = "https://github.com/kunobi-ninja/kache/releases/download/v${version}/kache-${source.target}.tar.gz";
    inherit (source) hash;
  };
  sourceRoot = ".";
  dontBuild = true;
  # Keep the upstream static Linux binary and Darwin signature intact.
  dontFixup = true;
  installPhase = ''
    runHook preInstall
    install -Dm755 kache $out/bin/kache
    ${lib.optionalString stdenvNoCC.isDarwin ''
      install -Dm755 ${../../../dev/kache-darwin-wrapper} $out/bin/xmtp-kache
      substituteInPlace $out/bin/xmtp-kache \
        --replace-fail '#!/usr/bin/env bash' '#!${bash}/bin/bash'
    ''}
    runHook postInstall
  '';
  doInstallCheck = true;
  installCheckPhase = ''
    $out/bin/kache --version | grep -F '1.0.0'
  '';
  meta = {
    description = "Content-addressed compiler cache for Rust and C/C++";
    homepage = "https://github.com/kunobi-ninja/kache";
    license = lib.licenses.asl20;
    mainProgram = "kache";
    platforms = builtins.attrNames sources;
  };
}
