# Thin wrapper around writeShellApplication for Babashka scripts.
# `text` is materialized to a .clj in the store; `classpath` entries
# are joined and passed to bb.
{
  writeShellApplication,
  babashka,
  lib,
}:
{
  name,
  text,
  classpath ? [ ],
  runtimeInputs ? [ ],
}:
let
  script = builtins.toFile "${name}.clj" text;
  cpArg = lib.optionalString (
    classpath != [ ]
  ) ''--classpath "${lib.concatStringsSep ":" classpath}" '';
in
writeShellApplication {
  inherit name;
  runtimeInputs = [ babashka ] ++ runtimeInputs;
  text = ''exec bb ${cpArg}${script} "$@"'';
}
