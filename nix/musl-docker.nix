# Musl overrides for rust crates, primarily for small docker builds
_: {
  perSystem =
    {
      self',
      pkgs,
      lib,
      config,
      ...
    }:
    let
      muslPkgs = pkgs.pkgsCross.musl64;
      aarch64Pkgs = pkgs.pkgsCross.aarch64-multiplatform-musl;

      commonStatic = old: pkgsCross: {
        CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
        OPENSSL_STATIC = "true";
        OPENSSL_LIB_DIR = "${pkgsCross.pkgsStatic.openssl.out}/lib";
        OPENSSL_INCLUDE_DIR = "${pkgsCross.pkgsStatic.openssl.dev}/include";
        OPENSSL_NO_VENDOR = "1";
        PKG_CONFIG_ALL_STATIC = "true";
        PKG_CONFIG_PATH = "${pkgsCross.pkgsStatic.openssl.dev}/lib/pkgconfig";
        depsBuildBuild = (old.depsBuildBuild or [ ]) ++ [ pkgsCross.stdenv.cc ];
        buildInputs = (old.buildInputs or [ ]) ++ [
          pkgsCross.pkgsStatic.openssl.dev
        ];
      };

      env-musl =
        old:
        {
          CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
          CC_x86_64_unknown_linux_musl = "${muslPkgs.stdenv.cc}/bin/${muslPkgs.stdenv.cc.targetPrefix}cc";
          CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${muslPkgs.stdenv.cc}/bin/${muslPkgs.stdenv.cc.targetPrefix}cc";
        }
        // (commonStatic old muslPkgs);

      env-aarch64-multiplatform =
        old:
        {
          CARGO_BUILD_TARGET = "aarch64-unknown-linux-musl";
          CC_aarch64_unknown_linux_musl = "${aarch64Pkgs.stdenv.cc}/bin/${aarch64Pkgs.stdenv.cc.targetPrefix}cc";
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER = "${aarch64Pkgs.stdenv.cc}/bin/${aarch64Pkgs.stdenv.cc.targetPrefix}cc";
        }
        // (commonStatic old aarch64Pkgs);

      imageCommon = {
        name = "ghcr.io/xmtp/mls-validation-service"; # override ghcr images
        tag = "main";
        created = "now";
      };
    in
    {
      packages = {
        mls-validation-service-musl64 =
          config.rust-project.crates.mls_validation_service.crane.outputs.drv.crate.overrideAttrs
            (old: old // (env-musl old));
        mls-validation-service-aarch64-multiplatform =
          config.rust-project.crates.mls_validation_service.crane.outputs.drv.crate.overrideAttrs
            (old: old // (env-aarch64-multiplatform old));

        # lib.recursiveUpdate lets imageCommon define other attributes in the `config` namesapce
        validation-service-image-musl64 = pkgs.dockerTools.buildLayeredImage (
          lib.recursiveUpdate imageCommon {
            config.entrypoint = [
              "${self'.packages.mls-validation-service-musl64}/bin/mls-validation-service"
            ];
          }
        );
        validation-service-image-aarch64-multiplatform = pkgs.dockerTools.buildLayeredImage (
          lib.recursiveUpdate imageCommon {
            config.entrypoint = [
              "${self'.packages.mls-validation-service-aarch64-multiplatform}/bin/mls-validation-service"
            ];
          }
        );
        validation-service-image = pkgs.dockerTools.buildLayeredImage (
          lib.recursiveUpdate imageCommon {
            config.entrypoint = [ "${self'.packages.mls_validation_service}/bin/mls-validation-service" ];
          }
        );
      };
    };
}
