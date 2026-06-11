{
  stdenv,
  darwin,
  mktemp,
  buf,
  curl,
  git,
  geckodriver,
  corepack,
  pkg-config,
  playwright-driver,
  playwright,
  lib,
  mkShell,
}:

mkShell {
  meta.description = "Javascript/BrowserSDK Development Environment";
  PLAYWRIGHT_BROWSERS_PATH = "${playwright-driver.browsers}";
  PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "true";
  PLAYWRIGHT_VERSION = "${playwright.version}";
  name = "xmtp-js environment";
  nativeBuildInputs = [ pkg-config ];
  buildInputs = [
    mktemp
    buf
    curl
    # eval-time builtins.fetchGit needs a git matching the nix glibc
    git
    geckodriver
    # playwright version here must match that in package.json EXACTLY for integration tests to work
    playwright
    playwright-driver.browsers
    corepack
  ]
  ++ lib.optionals stdenv.isDarwin [
    darwin.cctools
  ];

  VITE_PROJECT_ID = "2ca676e2e5e9322c40c68f10dca637e5";
}
