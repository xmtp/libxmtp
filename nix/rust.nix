# Rust Binaries to expose in nix flake
{ inputs, ... }:
{
  perSystem =
    { inputs', self', pkgs, config, lib, ... }:
    let
      fenix = inputs'.fenix.packages;
      rust = fenix.fromManifestFile inputs.rust-manifest;
      toolchain = fenix.combine [
        (fenix.targets."x86_64-unknown-linux-musl".fromManifestFile
          inputs.rust-manifest).rust-std
        rust.defaultToolchain
        rust."clippy"
        rust."rust-docs"
        rust."rustfmt-preview"
        rust."clippy-preview"
      ];
      src = ./..;
    in
    {
      packages.musl-mls_validation_service =
        config.rust-project.crates.mls_validation_service.crane.outputs.drv.crate.overrideAttrs
          (old:
            old // {
              CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
              CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
            }
          );
      rust-project = {
        inherit toolchain;
        # Override the default src to use our workspace fileset which includes
        # non-Cargo files like proto_descriptor.bin
        src = lib.fileset.toSource {
          root = ./..;
          fileset = (pkgs.xmtp.filesets { inherit lib; craneLib = config.rust-project.crane-lib; }).workspace;
        };
        defaults = {
          perCrate.crane.args = {
            doCheck = false;
            nativeBuildInputs = with pkgs;
              [
                pkg-config
                perl
                openssl
                sqlite
                sqlcipher
              ];
          };
        };
        crates = {
          "mls_validation_service" = {
            path = src + /apps/mls_validation_service;
            autoWire = [ "crate" ];
          };
          "xmtp_debug" = {
            autoWire = [ "crate" ];
            path = src + /crates/xmtp_debug;
          };
          "xmtp_cli" = {
            path = src + /apps/cli;
            autoWire = [ "crate" ];
          };
          "bindings_wasm" = {
            # wasm bindings have custom build in wasm.nix
            autoWire = [ ];
          };
        };
      };
      packages.default = self'.packages.xdbg;
    };
}
