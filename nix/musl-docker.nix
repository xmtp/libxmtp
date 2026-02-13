# Musl overrides for rust crates, primarily for small docker builds
_: {
  perSystem =
    {
      self',
      pkgs,
      config,
      lib,
      ...
    }:
    let
      muslPkgs = pkgs.pkgsCross.musl64;
      muslEnv = old: {
        CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
        CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
        depsBuildBuild = (old.depsBuildBuild or [ ]) ++ [ muslPkgs.stdenv.cc ];
        buildInputs = (old.buildInputs or [ ]) ++ [
          muslPkgs.pkgsStatic.openssl.dev
        ];
        CC_x86_64_unknown_linux_musl = "${muslPkgs.stdenv.cc}/bin/${muslPkgs.stdenv.cc.targetPrefix}cc";
        CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${muslPkgs.stdenv.cc}/bin/${muslPkgs.stdenv.cc.targetPrefix}cc";
        OPENSSL_STATIC = "true";
        OPENSSL_LIB_DIR = "${muslPkgs.pkgsStatic.openssl.out}/lib";
        OPENSSL_INCLUDE_DIR = "${muslPkgs.pkgsStatic.openssl.dev}/include";
        OPENSSL_NO_VENDOR = "1";
        PKG_CONFIG_ALL_STATIC = "true";
        PKG_CONFIG_PATH = "${muslPkgs.pkgsStatic.openssl.dev}/lib/pkgconfig";
      };
    in
    {
      packages = {

        mls-validation-service-musl64 =
          config.rust-project.crates.mls_validation_service.crane.outputs.drv.crate.overrideAttrs
            (old: old // (muslEnv old));
        # a very small docker build with anvil pre-populated for use in CI
        validation-service-image-musl64 = pkgs.dockerTools.buildLayeredImage {
          name = "ghcr.io/xmtp/mls-validation-service"; # override ghcr images
          tag = "main";
          created = "now";
          config = {
            Env = [
              "ANVIL_URL=http://anvil:8545"
            ];
            entrypoint = [ "${self'.packages.mls-validation-service-musl64}/bin/mls-validation-service" ];
          };
        };
      };
    };
}
