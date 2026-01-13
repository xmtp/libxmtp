# Rust Binaries to expose in nix flake
{ inputs, ... }:
{
  debug = true;
  perSystem =
    { inputs', lib, config, self', pkgs, ... }:
    let
      rust = inputs'.fenix.packages.fromManifestFile inputs.rust-manifest;
      toolchain = inputs'.fenix.packages.combine [
        rust.defaultToolchain
        rust."clippy"
        rust."rust-docs"
        rust."rustfmt-preview"
        rust."clippy-preview"
      ];
      common = with pkgs; [
        pkg-config
        perl
        openssl
      ];
      craneLib = config.rust-project.crane-lib;
      workspaceFileset = lib.fileset.toSource {
        root = ./..;
        fileset = (pkgs.xmtp.filesets { inherit lib craneLib; }).workspace;
      };
      extraArgs = {
        extraBuildArgs = {
          src = workspaceFileset;
        };
      };
    in
    {
      rust-project.crates = {
        "xdbg" = {
          path = ./../crates/xmtp_debug;
          crane = {
            args = {
              nativeBuildInputs = [ ] ++ common;
            };
          } // extraArgs;
        };
        "xli" = {
          path = ./../apps/cli;
          crane = {
            args = {
              nativeBuildInputs = [ ] ++ common;
            };
          } // extraArgs;
        };
      };

      rust-project = {
        inherit toolchain;
      };
      packages.default = self'.packages.xdbg;
    };
}
