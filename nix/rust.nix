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
      toolchain =
        pkgs.xmtp.mkToolchain pkgs
          [
            "x86_64-unknown-linux-musl"
            "aarch64-unknown-linux-musl"
          ]
          [ ];
      src = ./..;
      xnetFileset =
        crate:
        lib.fileset.toSource {
          root = ./..;
          fileset = lib.fileset.unions [
            pkgs.xmtp.filesets.libraries
            (pkgs.xmtp.craneLib.fileset.commonCargoSources (src + /apps/xnet/lib))
            crate
          ];
        };
    in
    {
      packages = {
        default = self'.packages.xdbg;
      };
      rust-project = {
        inherit toolchain;
        # Override the default src to use our workspace fileset which includes
        # non-Cargo files like proto_descriptor.bin (used for building dependencies)
        src = lib.fileset.toSource {
          root = ./..;
          fileset = pkgs.xmtp.filesets.workspace;
        };
        defaults = {
          perCrate.crane.args = pkgs.xmtp-base.commonArgs;
        };
        crates = {
          "xmtp_debug" = {
            autoWire = [ "crate" ];
            path = src + /apps/xmtp_debug;
            crane.args.nativeBuildInputs = pkgs.xmtp-base.commonArgs.nativeBuildInputs ++ [ pkgs.cacert ];
          };
          "xmtp_cli" = {
            path = src + /apps/cli;
            autoWire = [ "crate" ];
          };
          "xnet" = {
            path = src + /apps/xnet/lib;
            autoWire = [ "crate" ];
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
