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
      mkMlsValidationService = p: p.callPackage ./package/mls_validation_service.nix;
      mkBackend = p: p.callPackage ./package/backend.nix;
      backendImage =
        target: architecture:
        pkgs.dockerTools.buildLayeredImage {
          name = "ghcr.io/xmtp/backend";
          tag = "self-hosted";
          inherit architecture;
          contents = [ pkgs.cacert ];
          config.Entrypoint = [ "${self'.packages.${"xmtp-backend-${target}"}}/bin/xmtp-backend" ];
        };

      imageCommon = {
        name = "ghcr.io/xmtp/mls-validation-service"; # override ghcr images
        tag = "main";
        created = "now";
      };
    in
    {
      packages = {
        xmtp-backend = pkgs.callPackage ./package/backend.nix { };
        backend-image = backendImage "x86_64-unknown-linux-musl" "amd64";
        backend-image-x86_64-unknown-linux-musl = self'.packages.backend-image;
        backend-image-aarch64-unknown-linux-musl = backendImage "aarch64-unknown-linux-musl" "arm64";
        mls-validation-service = pkgs.callPackage ./package/mls_validation_service.nix { };
        # lib.recursiveUpdate lets imageCommon define other attributes in the `config` namesapce
        validation-service-image = pkgs.dockerTools.buildLayeredImage (
          lib.recursiveUpdate imageCommon {
            config.entrypoint = [
              "${self'.packages.mls-validation-service-x86_64-unknown-linux-musl}/bin/mls-validation-service"
            ];
            contents = [ pkgs.cacert ];
            architecture = "amd64";
          }
        );
        validation-service-image-aarch64-unknown-linux-musl = pkgs.dockerTools.buildLayeredImage (
          lib.recursiveUpdate imageCommon {
            config.entrypoint = [
              "${self'.packages.mls-validation-service-aarch64-unknown-linux-musl}/bin/mls-validation-service"
            ];
            contents = [ pkgs.cacert ];
          }
        );
      }
      # create mls validation service for all the cross compilation targets
      // lib.mapAttrs' (target: crossPkgs: {
        name = "mls-validation-service-${target}";
        value = mkMlsValidationService crossPkgs { };
      }) crossPkgs
      // lib.mapAttrs' (target: crossPkgs: {
        name = "xmtp-backend-${target}";
        value = mkBackend crossPkgs { };
      }) crossPkgs;
    };
}
