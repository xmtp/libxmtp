# Shared driver helpers for cross-version-test and cross-talk-test.
# Implemented in Clojure (Babashka). The bb runtime is wrapped via
# makeWrapper so callers invoke `xdbg-driver-lib` without knowing it's
# bb under the hood.
{
  mkBabashkaApp,
  git,
  jq,
  nix,
  coreutils,
}:
mkBabashkaApp {
  name = "xdbg-driver-lib";
  text = builtins.readFile ./xdbg_driver_lib.clj;
  srcDirs = {
    src = ./src;
    test = ./test;
  };
  extraSources = {
    "bb.edn" = ./bb.edn;
  };
  runtimeInputs = [
    git
    jq
    nix
    coreutils
  ];
}
