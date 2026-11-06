# Nix

Build affected outputs with `nix build --no-link .#<output>`; run
`dev/nix-shell 'just lint-config'` and `dev/nix-shell 'just test dev'`.

- Keep compiler credentials and writable host caches out of derivations.
- For shell logging/cache diagnostics, read [shell diagnostics](DEVELOPMENT.md#shell-diagnostics).
  Before changing Node pins or patches, read [runtime maintenance](node-runtimes.md).
- Keep source closures narrow but complete, including embedded data, migrations,
  proto inputs, and test fixtures. Use `xmtp.filesets.mkClosureSource` for package
  checks. Verify relevant edits change `.drvPath` and unrelated edits do not.
- Dependency layers must use their consumer's Cargo packages, targets, profile,
  features, and environment. Workspace feature unification can hide missing inputs.
- Keep compiler, linker, and Apple SDK from one toolchain. Preserve exact canonical
  Xcode pins and separate local outputs; never switch global Xcode in build code.
- Read [build maintenance](DEVELOPMENT.md) for source filters, dependency layers,
  services, bindgen pins, generated products, and Android emulator changes.
- Read [RN build constraints](REACT_NATIVE.md) before changing RN derivations,
  native dependencies, host access, or cross-host exchange. Keep sandbox access
  narrow and local receipts distinct from canonical admission. Keep advisory RN
  outputs isolated in `ciSets.<system>.rn`.
- Cache setup and limits: [Nix setup](../docs/nix-setup.md).
