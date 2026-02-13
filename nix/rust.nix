# Rust Binaries to expose in nix flake
_: {
  perSystem =
    {
      self',
      pkgs,
      lib,
      ...
    }:
    let
      toolchain = pkgs.xmtp.mkToolchain [ ] [ ];
      src = ./..;
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
            nativeBuildInputs = with pkgs; [
              pkg-config
              openssl
              perl
              sqlite
              sqlcipher
            ];
          };
        };
        crates = {
          "mls_validation_service" = {
            path = src + /apps/mls_validation_service;
            autoWire = [ ];
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
