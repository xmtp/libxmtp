{ shells
, stdenv
, darwin
, mktemp
, buf
, curl
, geckodriver
, corepack
, pkg-config
, playwright-driver
, playwright
, lib
, yalc
,
}:

let
  inherit (darwin.apple_sdk) frameworks;
  mkShell =
    top:
    (shells.combineShell
      {
        otherShells = with shells;
          [
            mkLinters
          ];
        extraInputs = top;
      });
in
mkShell {
  PLAYWRIGHT_BROWSERS_PATH = "${playwright-driver.browsers}";
  PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "true";
  PLAYWRIGHT_VERSION = "${playwright.version}";
  name = "xmtp-js environment";
  nativeBuildInputs = [ pkg-config ];
  buildInputs =
    [
      mktemp
      buf
      curl
      geckodriver
      playwright
      playwright-driver.browsers
      yalc
      corepack
    ]
    ++ lib.optionals stdenv.isDarwin [
      frameworks.CoreServices
      frameworks.Carbon
      frameworks.ApplicationServices
      frameworks.AppKit
      darwin.cctools
    ];
  VITE_PROJECT_ID = "2ca676e2e5e9322c40c68f10dca637e5";
}
