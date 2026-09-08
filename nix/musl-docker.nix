# Musl overrides for rust crates, primarily for small docker builds
{ self, ... }:
{
  perSystem =
    {
      self',
      pkgs,
      lib,
      system,
      ...
    }:
    let
      targets = [
        "x86_64-unknown-linux-musl"
        "aarch64-unknown-linux-musl"
      ];

      crossPkgs = self.lib.mkCrossPkgs system targets;
      mkBackend = p: p.callPackage ./package/backend.nix;
      backendImage =
        target: architecture:
        pkgs.dockerTools.buildLayeredImage {
          name = "ghcr.io/xmtp/backend";
          tag = "self-hosted";
          inherit architecture;
          contents = [
            pkgs.cacert
            crossPkgs.${target}.grpc-health-probe
          ];
          config.Entrypoint = [ "${self'.packages.${"xmtp-backend-${target}"}}/bin/xmtp-backend" ];
        };

    in
    {
      packages = {
        xmtp-backend = pkgs.callPackage ./package/backend.nix { };
        backend-image = backendImage "x86_64-unknown-linux-musl" "amd64";
        backend-image-x86_64-unknown-linux-musl = self'.packages.backend-image;
        backend-image-aarch64-unknown-linux-musl = backendImage "aarch64-unknown-linux-musl" "arm64";
      }
      // lib.mapAttrs' (target: crossPkgs: {
        name = "xmtp-backend-${target}";
        value = mkBackend crossPkgs { };
      }) crossPkgs;
    };
}
