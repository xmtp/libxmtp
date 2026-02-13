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
      muslEnv = old: {
        CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
        CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS = "-C target-feature=+crt-static";
      };
    in
    {
      # musl-mls_validation_service builds just fine on darwin,
      # it just takes forever b/c it compiles the musl gcc for aarch64 darwin from scratch
      # (need to add pkgs.pkgsCross.musl64 versions of `buildInputs` on darwin, & point to right CC)
      # restrict build to linux only to save some CI time
      # can still build on darwin via Qemu
      packages = lib.optionalAttrs pkgs.stdenv.isLinux {
        musl-mls_validation_service =
          config.rust-project.crates.mls_validation_service.crane.outputs.drv.crate.overrideAttrs
            (old: old // (muslEnv old));
        # a very small docker build with anvil pre-populated for use in CI
        docker-mls_validation_service = pkgs.dockerTools.buildLayeredImage {
          name = "ghcr.io/xmtp/mls-validation-service"; # override ghcr images
          tag = "main";
          created = "now";
          config = {
            Env = [
              "ANVIL_URL=http://anvil:8545"
            ];
            entrypoint = [ "${self'.packages.musl-mls_validation_service}/bin/mls-validation-service" ];
          };
        };
      };
    };
}
