# Rust Binaries to expose in nix flake
_: {
  perSystem =
    { self'
    , pkgs
    , lib
    , ...
    }:
    let
      toolchain =
        pkgs.xmtp.mkToolchain
          [
            "x86_64-unknown-linux-musl"
            "aarch64-unknown-linux-musl"
          ]
          [ ];
      src = ./..;
      xnetFileset = crate: lib.fileset.toSource {
        root = ./..;
        fileset = lib.fileset.unions [
          pkgs.xmtp.filesets.libraries
          (pkgs.xmtp.craneLib.fileset.commonCargoSources (src + /apps/xnet/lib))
          crate
        ];
      };
      callPackage = lib.callPackageWith
        (pkgs // {
          inherit xnetFileset;
        });
    in
    {
      packages = {
        xnet-gui = (callPackage ./package/xnet-gui.nix { }).bin;
        default = self'.packages.xdbg;
      };
      devShells.xnet-gui = (callPackage ./package/xnet-gui.nix { }).devShell;
      rust-project = {
        inherit toolchain;
        # Override the default src to use our workspace fileset which includes
        # non-Cargo files like proto_descriptor.bin (used for building dependencies)
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
          "xnet" = {
            path = src + /apps/xnet/lib;
            autoWire = [ "crate" ];
          };
          "xnet-gui" = {
            path = src + /apps/xnet/gui;
            autoWire = [ ];
          };
          "xnet-cli" = {
            crane.args.src = xnetFileset (src + /apps/xnet/cli);
            path = src + /apps/xnet/cli;
            autoWire = [ "crate" ];
          };
        };
      };
    };
}
