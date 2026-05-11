# Derivation that packages the cross-version-test driver as a
# self-contained shell application with declared runtime deps. The
# script lives alongside this file (cross-version-test.sh); this is
# just the Nix wrapper so callers don't need to know it's bash.
#
# writeShellApplication runs shellcheck at build time, so a build
# failure here means the script needs fixing before merge.
{
  writeShellApplication,
  git,
  jq,
  coreutils,
  gnugrep,
  gnused,
  gawk,
  util-linux,
  nix,
}:
writeShellApplication {
  name = "cross-version-test";
  runtimeInputs = [
    git
    jq
    coreutils
    gnugrep
    gnused
    gawk
    util-linux # provides column(1), used in the run-sequence plan-table.
    # `nix` itself is needed because the script invokes
    # `nix run github:xmtp/libxmtp/<sha>#xdbg` for each version.
    # writeShellApplication prepends runtimeInputs to PATH, so this pins a
    # specific Nix binary; that's fine for CI and Nix-shell users (the only
    # contexts this tool runs in), and ensures the script doesn't break if
    # the ambient PATH lacks nix.
    nix
  ];
  text = builtins.readFile ./cross-version-test.sh;
}
