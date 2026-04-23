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
      callPackage = lib.callPackageWith (
        pkgs
        // {
          inherit xnetFileset;
        }
      );
    in
    {
      packages = {
        default = self'.packages.xdbg;
      };
      # xnet-gui is exposed via legacyPackages (instead of packages/devShells) so
      # it is excluded from the Cache all Nix Outputs workflow. `om ci run` uses
      # devour-flake, which walks packages/devShells/checks/apps but ignores
      # arbitrary legacyPackages attrs. This avoids flaky darwin build failures
      # caused by xnet-gui's GUI toolchain dependencies (apple-sdk, harfbuzz,
      # fontconfig) without disabling the package outright. Both
      # `nix build .#xnet-gui` and `nix develop .#xnet-gui-shell` continue to
      # work via Nix's legacyPackages fallback, so release-xnet-gui.yml needs
      # no changes.
      legacyPackages = {
        xnet-gui = (callPackage ./package/xnet-gui.nix { }).bin;
        xnet-gui-shell = (callPackage ./package/xnet-gui.nix { }).devShell;
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
          perCrate.crane.args = pkgs.xmtp.base.commonArgs;
        };
        crates = {
          "xmtp_debug" = {
            autoWire = [ "crate" ];
            path = src + /apps/xmtp_debug;
            # cacert is needed so xdbg can negotiate TLS to remote XMTP
            # gateways at runtime (e.g. grpc.testnet.xmtp.network) without
            # an externally-provided CA bundle.  Concatenate onto the
            # shared commonArgs.nativeBuildInputs rather than replace it.
            crane.args.nativeBuildInputs = pkgs.xmtp.base.commonArgs.nativeBuildInputs ++ [ pkgs.cacert ];
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
