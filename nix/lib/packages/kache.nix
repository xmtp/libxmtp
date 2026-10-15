# Official release archives. Refresh all hashes when the version changes, and
# move the `version` input of kunobi-ninja/kache-action in
# .github/actions/setup-nix/action.yml with it (dev/tests/test_kache_env.py).
{
  lib,
  stdenvNoCC,
  fetchurl,
  bash,
}:
let
  version = "1.2.2";
  sources = {
    aarch64-darwin = {
      target = "aarch64-apple-darwin";
      hash = "sha256-ENCeFGhSqN+Hls9kLKWIchB6fzkbdkfsjMJTzyia8Kk=";
    };
    x86_64-darwin = {
      target = "x86_64-apple-darwin";
      hash = "sha256-mhw+zInPNrI1Eqk4aCJhQNRvH3DPbLZIv+CO6vslo2g=";
    };
    aarch64-linux = {
      target = "aarch64-unknown-linux-musl";
      hash = "sha256-OIVs7mE/buDsV5CZ/JIFuECgtFu3Y/S4+ngtJdk8GO8=";
    };
    x86_64-linux = {
      target = "x86_64-unknown-linux-musl";
      hash = "sha256-8vZVXe5QlFwqkKf7eTwghKAMxkWdCKwjBtONkFBkMhs=";
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
      install -Dm755 ${./kache-darwin-wrapper.sh} $out/bin/xmtp-kache
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
