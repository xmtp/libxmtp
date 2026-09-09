{
  mkShell,
  lib,
  stdenv,
  nodejs_24,
  corepack,
  just,
  markdownlint-cli,
  playwright-driver,
  playwright,
}:
mkShell {
  name = "xmtp-docs";
  packages = [
    nodejs_24
    corepack
    just
    markdownlint-cli
  ];
  PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
  PLAYWRIGHT_VERSION = playwright.version;
  shellHook = lib.optionalString stdenv.isLinux ''
    export PLAYWRIGHT_BROWSERS_PATH=${playwright-driver.browsers}
  '';
}
