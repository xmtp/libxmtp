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

      imageCommon = {
        name = "ghcr.io/xmtp/mls-validation-service"; # override ghcr images
        tag = "main";
        created = "now";
      };
    in
    {
      packages = {
        mls-validation-service = pkgs.callPackage ./package/mls_validation_service.nix { };
        # lib.recursiveUpdate lets imageCommon define other attributes in the `config` namesapce
        validation-service-image = pkgs.dockerTools.buildLayeredImage (
          lib.recursiveUpdate imageCommon {
            config.entrypoint = [
              "${self'.packages.mls-validation-service-x86_64-unknown-linux-musl}/bin/mls-validation-service"
            ];
            architecture = "amd64";
          }
        );
        validation-service-image-aarch64-unknown-linux-musl = pkgs.dockerTools.buildLayeredImage (
          lib.recursiveUpdate imageCommon {
            config.entrypoint = [
              "${self'.packages.mls-validation-service-aarch64-unknown-linux-musl}/bin/mls-validation-service"
            ];
          }
        );
      }
      # create mls validation service for all the cross compilation targets
      // lib.mapAttrs' (target: crossPkgs: {
        name = "mls-validation-service-${target}";
        value = mkMlsValidationService crossPkgs { };
      }) crossPkgs;
    };
}
