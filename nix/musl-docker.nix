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
      # x86_64-musl is available on all systems (same-arch cross for x86_64-linux)
      x86Targets = [ "x86_64-unknown-linux-musl" ];
      # aarch64-musl is only available on aarch64 systems (same-arch cross)
      aarch64Targets = [ "aarch64-unknown-linux-musl" ];

      x86CrossPkgs = self.lib.mkCrossPkgs system x86Targets;
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
      }
      # x86_64 musl cross-compilation (available on all systems)
      // lib.mapAttrs' (target: crossPkgs: {
        name = "mls-validation-service-${target}";
        value = mkMlsValidationService crossPkgs { };
      }) x86CrossPkgs
      # aarch64 musl cross-compilation + docker image (aarch64 systems only)
      // lib.optionalAttrs (lib.hasPrefix "aarch64" system) (
        let
          aarch64CrossPkgs = self.lib.mkCrossPkgs system aarch64Targets;
        in
        (lib.mapAttrs' (target: crossPkgs: {
          name = "mls-validation-service-${target}";
          value = mkMlsValidationService crossPkgs { };
        }) aarch64CrossPkgs)
        // {
          validation-service-image-aarch64-unknown-linux-musl = pkgs.dockerTools.buildLayeredImage (
            lib.recursiveUpdate imageCommon {
              config.entrypoint = [
                "${self'.packages.mls-validation-service-aarch64-unknown-linux-musl}/bin/mls-validation-service"
              ];
            }
          );
        }
      );
    };
}
