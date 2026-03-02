{
  inputs,
  lib,
  fenix,
  ...
}:
let
  toolchain = (fenix.fromManifestFile inputs.rust-manifest).defaultToolchain;
in
targets: components:
fenix.combine [
  toolchain
  (lib.forEach targets (
    target: (fenix.targets."${target}".fromManifestFile inputs.rust-manifest).rust-std
  ))
  (lib.forEach components (component: (fenix.fromManifestFile inputs.rust-manifest)."${component}"))
]
