# yarn support in nix isn't theb est right now, but its getting better
# Issue: https://github.com/NixOS/nixpkgs/issues/254369#issuecomment-2080460150
{ fetchFromGitHub
, stdenvNoCC
, yarn-berry
, nodejs
, cacert
}:

stdenvNoCC.mkDerivation (finalAttrs: {
  pname = "@xmtp/browser-sdk";
  version = "0.0.21";

  src = fetchFromGitHub {
    owner = "xmtp";
    repo = "xmtp-js";
    rev = "main";
    sha256 = "sha256-PLNnS6oxdH4wlgmTQsiObJoLng0AsTCk2yOm3yX5RXU=";
  };

  nativeBuildInputs = [ yarn-berry nodejs ];

  yarnOfflineCache = stdenvNoCC.mkDerivation {
    name = "xmtp-js-deps";
    nativeBuildInputs = [ yarn-berry ];
    inherit (finalAttrs) src;

    NODE_EXTRA_CA_CERTS = "${cacert}/etc/ssl/certs/ca-bundle.crt";

    supportedArchitectures = builtins.toJSON {
      os = [ "darwin" "linux" ];
      cpu = [ "arm" "arm64" "ia32" "x64" ];
      libc = [ "glibc" "musl" ];
    };

    configurePhase = ''
      runHook preConfigure

      export HOME="$NIX_BUILD_TOP"
      export YARN_ENABLE_TELEMETRY=0

      yarn config set enableGlobalCache false
      yarn config set cacheFolder $out
      yarn config set supportedArchitectures --json "$supportedArchitectures"

      runHook postConfigure
    '';

    buildPhase = ''
      runHook preBuild

      mkdir -p $out
      yarn install --immutable --mode skip-build

      runHook postBuild
    '';

    dontInstall = true;

    outputHashAlgo = "sha256";
    outputHash = "sha256-K+2PwQYKZgMZ4B87BPUbuYYDMxMxhJI7KKAEFsZ/YBE=";
    outputHashMode = "recursive";
  };

  configurePhase = ''
    runHook preConfigure

    export HOME="$NIX_BUILD_TOP"
    export YARN_ENABLE_TELEMETRY=0

    yarn config set enableGlobalCache false
    yarn config set cacheFolder $yarnOfflineCache

    runHook postConfigure
  '';

  buildPhase = ''
    runHook preBuild

    yarn install --immutable --immutable-cache
    yarn build
    yarn workspaces focus --all --production

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out/{lib}

    cp -r node_modules $out/lib/

    for sdk in sdks/*; do
      mkdir -p $out/lib/$sdk
      cp -r $sdk/{dist,src,package.json} $out/lib/$sdk/
    done

    runHook postInstall
  '';

  fixupPhase = ''
    runHook preFixup

    patchShebangs $out/lib

    runHook postFixup
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    # $out/bin/spectral --version
    yarn --cwd $out/lib/sdks/browser-sdk test
  '';
})
