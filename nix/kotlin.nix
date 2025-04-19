# This build will only work on darwin
{ pkg-config
, mkShell
, jdk11_headless
, maven
, buildMavenRepositoryFromLockFile
, kotlin
, mkToolchain
, cargo-nextest
, ...
}:

let
  repository = buildMavenRepositoryFromLockFile { file = ../bindings_ffi/tests/mvn2nix-lock.json; };
  rust-toolchain = mkToolchain [ ] [ ];
in
mkShell {
  nativeBuildInputs = [ pkg-config jdk11_headless maven ];
  buildInputs = [ repository kotlin rust-toolchain cargo-nextest ];
  buildPhase = ''
    mvn --file ../bindings_ffi/tests/ --offline -Dmaven.repo.local=${repository} package
  '';

  shellHook = ''
    export CLASSPATH="$(find ${repository} -name "*.jar" -printf ':%h/%f')"
  '';
}
