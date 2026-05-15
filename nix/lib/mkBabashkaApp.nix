# mkBabashkaApp — package a Babashka (Clojure) script as a Nix derivation.
#
# Modeled on `writeShellApplication` / `writeBabashkaApplication`: the
# entry script is embedded as a string rather than living in a source
# directory the derivation has to `cp`. Auxiliary multi-file libraries
# go in `extraSources` as Nix paths, which are imported via the store
# (no unpackPhase, no `src = ./.;`).
#
# Standardizes the three things every bb driver in this repo wants:
#   1. write the entry .clj + any auxiliary files under $out/lib
#   2. wrap babashka so callers invoke `<name>` (no .clj suffix, no manual
#      --classpath, no PATH ceremony)
#   3. parse-check every .clj at build time so syntax errors fail the
#      derivation instead of the first runtime invocation
#
# Arguments:
#   name              derivation name + installed binary name
#   text              entry script text. Usually `builtins.readFile ./foo.clj`.
#   version           optional, defaults to "0.1.0"
#   extraSources      attrset { "rel/path.clj" = ./local/path.clj; ... }
#                     of auxiliary files copied next to the entry script.
#                     Each .clj is also parse-checked at build time.
#   srcDirs           attrset { "src" = ./src; "test" = ./test; ... }
#                     of directories to import as classpath roots. Each
#                     becomes $out/lib/<key>/ at install time and a
#                     classpath entry.
#   classpath         list of extra classpath entries appended to the bb
#                     wrapper (e.g. [ "${xdbg-driver-lib}/lib/src" ]).
#                     $out/lib + every srcDirs key are always included.
#   runtimeInputs     list of derivations whose /bin is added to PATH via
#                     makeWrapper --prefix.
{
  stdenvNoCC,
  lib,
  makeWrapper,
  babashka,
  writeText,
}:
{
  name,
  text,
  version ? "0.1.0",
  extraSources ? { },
  srcDirs ? { },
  classpath ? [ ],
  runtimeInputs ? [ ],
}:
let
  mainFile = "${name}.clj";
  entryDrv = writeText mainFile text;
  srcDirNames = lib.attrNames srcDirs;
  installClasspath = [ "$out/lib" ] ++ map (d: "$out/lib/${d}") srcDirNames;
  classpathArg = lib.concatStringsSep ":" (installClasspath ++ classpath);

  # Generate one-liner per extraSource: `install -Dm644 <path> $out/lib/<rel>`.
  installExtraSources = lib.concatStringsSep "\n" (
    lib.mapAttrsToList (rel: path: ''install -Dm644 ${path} "$out/lib/${rel}"'') extraSources
  );
  installSrcDirs = lib.concatStringsSep "\n" (
    lib.mapAttrsToList (dirName: dirPath: ''cp -r ${dirPath} "$out/lib/${dirName}"'') srcDirs
  );

  # All .clj files we parse-check at build time. For srcDirs we glob the
  # whole tree; for extraSources we filter on the rel path.
  extraSourceCljFiles = lib.filterAttrs (rel: _: lib.hasSuffix ".clj" rel) extraSources;
  checkExtraSources = lib.concatStringsSep "\n" (
    lib.mapAttrsToList (
      _: path: ''${babashka}/bin/bb --classpath "$classpath" --prn ${path} > /dev/null''
    ) extraSourceCljFiles
  );
  checkSrcDirs = lib.concatStringsSep "\n" (
    lib.mapAttrsToList (_: dirPath: ''
      find ${dirPath} -name '*.clj' -print0 | while IFS= read -r -d "" f; do
        ${babashka}/bin/bb --classpath "$classpath" --prn "$f" > /dev/null
      done
    '') srcDirs
  );
in
stdenvNoCC.mkDerivation {
  pname = name;
  inherit version;
  dontUnpack = true;
  dontConfigure = true;
  dontBuild = true;

  nativeBuildInputs = [
    makeWrapper
    babashka
  ];

  # No source dir — phases generate $out content directly.
  doCheck = true;
  checkPhase = ''
    runHook preCheck
    # Build the check classpath: every srcDir's absolute store path +
    # any external classpath entries (which are already absolute).
    classpath="${lib.concatStringsSep ":" ((lib.mapAttrsToList (_: p: "${p}") srcDirs) ++ classpath)}"
    ${babashka}/bin/bb --classpath "$classpath" --prn ${entryDrv} > /dev/null
    ${checkExtraSources}
    ${checkSrcDirs}
    runHook postCheck
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p $out/bin $out/lib
    install -Dm644 ${entryDrv} "$out/lib/${mainFile}"
    ${installExtraSources}
    ${installSrcDirs}
    makeWrapper ${babashka}/bin/bb $out/bin/${name} \
      --add-flags "--classpath ${classpathArg}" \
      --add-flags "$out/lib/${mainFile}" \
      ${lib.optionalString (runtimeInputs != [ ]) "--prefix PATH : ${lib.makeBinPath runtimeInputs}"}
    runHook postInstall
  '';
}
