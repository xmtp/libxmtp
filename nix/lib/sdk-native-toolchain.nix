# Opaque compilation identity, plus the human-readable Apple pin. The digest
# binds the exact declared compiler, linker, SDK, flags, and dependency graph.
compilation: {
  build = builtins.hashString "sha256" (builtins.unsafeDiscardStringContext compilation.drvPath);
  rust = builtins.hashFile "sha256" ../../rust-toolchain.toml;
  apple = compilation.XMTP_XCODE or null;
}
