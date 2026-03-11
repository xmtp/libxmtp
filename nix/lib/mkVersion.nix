# Version extracted from workspace Cargo.toml — use this instead of calling
# crateNameFromCargoToml multiple times in each derivation.
# Note: This requires the caller to pass in a crane instance with the right toolchain.
rust:
(rust.crateNameFromCargoToml {
  cargoToml = ./../../Cargo.toml;
}).version
