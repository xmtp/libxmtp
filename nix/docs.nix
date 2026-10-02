{
  mkShell,
  lib,
  stdenv,
  nodejs_26,
  xmtp-pnpm,
  just,
  markdownlint-cli,
  lychee,
  playwright-driver,
  playwright,
}:
mkShell {
  name = "xmtp-docs";
  packages = [
    nodejs_26
    xmtp-pnpm
    just
    markdownlint-cli
    lychee
  ];
  PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
  PLAYWRIGHT_VERSION = playwright.version;
  shellHook = lib.optionalString stdenv.isLinux ''
    export PLAYWRIGHT_BROWSERS_PATH=${playwright-driver.browsers}
  '';
}
