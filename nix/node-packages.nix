{ inputs, self, ... }:
{
  perSystem =
    {
      pkgs,
      lib,
      system,
      ...
    }:
    let
      # Targets are split by host-platform availability.
      # - gnu targets require glibc cross-compilation, which is broken on macOS
      #   (darwin-cross-build.patch fails to apply). Build these only on Linux.
      # - musl targets use self-contained musl toolchains that work everywhere.
      # - Darwin targets require Apple SDKs, so macOS only.
      # - Windows is excluded (built separately in CI).
      linuxGnuTargets = [
        "x86_64-unknown-linux-gnu"
        "aarch64-unknown-linux-gnu"
      ];

      linuxMuslTargets = [
        "x86_64-unknown-linux-musl"
        "aarch64-unknown-linux-musl"
      ];

      darwinTargets = [
        "x86_64-apple-darwin"
        "aarch64-apple-darwin"
      ];

      nodeTargets =
        linuxMuslTargets
        ++ lib.optionals pkgs.stdenv.isLinux linuxGnuTargets
        ++ lib.optionals pkgs.stdenv.isDarwin darwinTargets;

      # overlay glibc for node
      crossPkgs = lib.genAttrs nodeTargets (
        target:
        (import inputs.nixpkgs (
          self.lib.pkgConfig
          // {
            localSystem = system;
            crossSystem = target;
          }
        ))
      );
      mkNodeBindings = p: p.callPackage ./package/node-bindings.nix;
    in
    {
      packages = {
        node-bindings-js = mkNodeBindings pkgs { withJs = true; };
        node-bindings-fast = mkNodeBindings pkgs { };
        node-bindings-test = mkNodeBindings pkgs {
          withJs = true;
          test = true;
        };
      }
      // lib.mapAttrs' (target: crossPkgs: {
        name = "node-bindings-${pkgs.xmtp.toNapiTarget target}";
        value = mkNodeBindings crossPkgs { };
      }) crossPkgs;
    };
}
