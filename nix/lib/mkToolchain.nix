{
  inputs,
  lib,
  ...
}:
let
  toolchain = p: (p.fenix.fromManifestFile inputs.rust-manifest).defaultToolchain;
in
# this p is incredibly important for correct cross-compilation behavior
# crane splices pkgs according to buildInputs/nativeBuildInputs
# and the toolchain must be built with the correct package set according to target.
p: targets: components:
p.fenix.combine [
  (toolchain p)
  (lib.forEach targets (
    target: (p.fenix.targets."${target}".fromManifestFile inputs.rust-manifest).rust-std
  ))
  (lib.forEach components (component: (p.fenix.fromManifestFile inputs.rust-manifest)."${component}"))
]
