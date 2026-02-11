# Rust Binaries to expose in nix flake
{ inputs, ... }:
{
  perSystem =
    { inputs', self', pkgs, config, lib, ... }:
    let
      src = ./..;
      # Use mkToolchain for consistent toolchain creation across the project.
      # Include musl target for cross-compilation support.
      toolchain = pkgs.xmtp.mkToolchain
        [ "x86_64-unknown-linux-musl" ]
        [ "clippy" "rust-docs" "rustfmt-preview" "clippy-preview" ];
    in
    {
      rust-project = {
        inherit toolchain;
        # Override the default src to use our workspace fileset which includes
        # non-Cargo files like proto_descriptor.bin
        src = lib.fileset.toSource {
          root = ./..;
          fileset = pkgs.xmtp.filesets.workspace;
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
