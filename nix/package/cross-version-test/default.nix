# cross-version-test driver. Delegates the bulk of its logic to
# xdbg-driver-lib via shared classpath; this derivation just bundles
# the script + wires the runtime deps.
{
  mkBabashkaApp,
  xmtp,
  git,
  jq,
  nix,
  coreutils,
}:
mkBabashkaApp {
  name = "cross-version-test";
  text = builtins.readFile ./cross_version_test.clj;
  classpath = [ "${xmtp.xdbg-driver-lib}/lib/src" ];
  runtimeInputs = [
    xmtp.xdbg-driver-lib
    git
    jq
    nix
    coreutils
  ];
}
