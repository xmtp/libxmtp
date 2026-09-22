{
  lib,
  fetchurl,
  pnpm_11,
  nodejs-slim_26,
}:
let
  manifest = builtins.fromJSON (builtins.readFile ../../../package.json);
  packageManager = manifest.packageManager or "";
  prefix = "pnpm@";
  version = lib.removePrefix prefix packageManager;
in
assert lib.hasPrefix prefix packageManager;
(pnpm_11.override { nodejs-slim = nodejs-slim_26; }).overrideAttrs (_: {
  inherit version;
  src = fetchurl {
    url = "https://registry.npmjs.org/pnpm/-/pnpm-${version}.tgz";
    hash = "sha512-qB1MIbmwmksK6/kO9eUn1CYr3aMi0mCCl4y1SQI6xtnK/Jixn/+qXHHbQ4pl9wIk7beNcxEYeGH/lkgNHNyiPA==";
  };
})
