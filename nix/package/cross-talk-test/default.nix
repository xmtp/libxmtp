{
  python3Packages,
  xmtp,
  git,
  nix,
  coreutils,
  lib,
}:
python3Packages.buildPythonApplication {
  pname = "cross-talk-test";
  version = "0.1.0";
  pyproject = true;
  src = ../../../dev/drivers/cross_talk_test;
  build-system = [ python3Packages.setuptools ];
  dependencies = [ xmtp.xdbg-driver-lib ];
  makeWrapperArgs = [
    "--prefix"
    "PATH"
    ":"
    (lib.makeBinPath [
      git
      nix
      coreutils
    ])
  ];
}
