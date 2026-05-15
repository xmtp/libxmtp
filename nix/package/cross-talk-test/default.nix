# cross-talk-test driver. Sibling to cross-version-test; the difference
# is that each version runs under --strict-versioning so identities are
# partitioned per version, testing wire-level MLS interop rather than
# SQLite upgrade compat.
{
  mkBabashkaApp,
  xmtp,
  git,
  jq,
  nix,
  coreutils,
}:
mkBabashkaApp {
  name = "cross-talk-test";
  text = builtins.readFile ./cross_talk_test.clj;
  classpath = [ "${xmtp.xdbg-driver-lib}/lib/src" ];
  runtimeInputs = [
    xmtp.xdbg-driver-lib
    git
    jq
    nix
    coreutils
  ];
}
