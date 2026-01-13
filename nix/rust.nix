# Rust Binaries to expose in nix flake
{ inputs, ... }:
{
  perSystem =
    { inputs', self', pkgs, config, lib, ... }:
    let
      rust = inputs'.fenix.packages.fromManifestFile inputs.rust-manifest;
      toolchain = inputs'.fenix.packages.combine [
        rust.defaultToolchain
        rust."clippy"
        rust."rust-docs"
        rust."rustfmt-preview"
        rust."clippy-preview"
      ];
      src = ./..;
    in
    {
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
          "xmtp_cli" = {
            path = src + /apps/cli;
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
