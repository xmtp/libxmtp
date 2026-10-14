# A shell step that installs nextest's JUnit report as `$out/junit.xml`. The
# `ci` profile writes it to `target/nextest/ci/junit.xml` under the workspace
# root (nextest `store.dir`). A failed attempt of a retried test keeps its
# panic text, which can name a vendored crate's store path, so `nuke-refs`
# rewrites every store hash, and the output still names no store path
# (`allowedReferences = [ ]`). The caller puts `nukeReferences` in its
# nativeBuildInputs. `junit-refs` (nix/ci-checks.nix) runs this step on a
# report that names a real input.
report: ''
  install -m 0644 ${report} "$out/junit.xml"
  nuke-refs "$out/junit.xml"
''
