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
      craneLib = config.rust-project.crane-lib;
      xnetFileset = crate: lib.fileset.toSource {
        root = ./..;
        fileset = lib.fileset.unions [
          (pkgs.xmtp.filesets { inherit lib craneLib; }).libraries
          (craneLib.fileset.commonCargoSources (src + /apps/xnet/lib))
          crate
        ];
      };
      callPackage = lib.callPackageWith
        (pkgs // {
          inherit craneLib xnetFileset;
        });
    in
    {
      packages = {
        # validation service compiled with musl to make ultra-small docker containers (12MB)
        musl-mls_validation_service =
          config.rust-project.crates.mls_validation_service.crane.outputs.drv.crate.overrideAttrs
            (old:
              old // {
                CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
                CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
              }
            );
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
          fileset = (pkgs.xmtp.filesets { inherit lib craneLib; }).libraries;
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
                zlib
                zstd
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
