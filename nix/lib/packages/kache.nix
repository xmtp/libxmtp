# Official release archives. Refresh all hashes when the version changes, and
# move the `version` input of kunobi-ninja/kache-action in
# .github/actions/setup-nix/action.yml with it (dev/agents/test_kache_env.py).
{
  lib,
  stdenvNoCC,
  fetchurl,
  bash,
}:
let
  version = "1.1.1";
  sources = {
    aarch64-darwin = {
      target = "aarch64-apple-darwin";
      hash = "sha256-5fWbhcMND8wtJ0OvdiZ3SzloD8sGQkOIe+1Dd+clxhE=";
    };
    x86_64-darwin = {
      target = "x86_64-apple-darwin";
      hash = "sha256-f1sdHcjPXrYHhurX1qBwnIet8cbvz2Aow7l1iYj1mvU=";
    };
    aarch64-linux = {
      target = "aarch64-unknown-linux-musl";
      hash = "sha256-PC6C9CCSEjqTcASqGjte+nZsObBx7tzi3p6bUfvpGwg=";
    };
    x86_64-linux = {
      target = "x86_64-unknown-linux-musl";
      hash = "sha256-LvCXEhuH7Xok+bQPv103mYN7TCrHU5HSJk6lzGw4s9c=";
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
    $out/bin/kache --version | grep -F '${version}'
  '';
  meta = {
    description = "Content-addressed compiler cache for Rust and C/C++";
    homepage = "https://github.com/kunobi-ninja/kache";
    license = lib.licenses.asl20;
    mainProgram = "kache";
    platforms = builtins.attrNames sources;
  };
}
