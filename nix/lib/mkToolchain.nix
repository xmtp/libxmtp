{ pkgs, inputs }:

targets: components: pkgs.fenix.combine [
  (pkgs.fenix.fromManifestFile inputs.rust-manifest).minimalToolchain
  (pkgs.lib.forEach targets (target: (pkgs.fenix.targets."${target}".fromManifestFile inputs.rust-manifest).rust-std))
  (pkgs.lib.forEach components (c: (pkgs.fenix.fromManifestFile inputs.rust-manifest)."${c}"))
]
