# Musl cross-compilation module.
# Builds statically-linked Linux binaries for x86_64-unknown-linux-musl,
# suitable for minimal Docker images (FROM scratch).
#
# Uses the idiomatic Nix cross-compilation approach with crossSystem,
# which provides a complete build environment where all dependencies
# (openssl, sqlite, etc.) are automatically built for the target platform.
#
# Cross-compilation is supported from both Linux and macOS hosts.
{ inputs, ... }:
{
  perSystem =
    { pkgs, lib, system, config, ... }:
    let
      muslTarget = "x86_64-unknown-linux-musl";

      # Import nixpkgs with crossSystem to get a complete cross-compilation
      # environment. This is the idiomatic Nix approach - all packages in
      # muslPkgs are built for x86_64-linux-musl.
      muslPkgs = import inputs.nixpkgs {
        localSystem = system;
        crossSystem = {
          config = muslTarget;
          isStatic = true;
        };
      };

      # Musl-specific environment variables for cross-compilation
      muslEnv = {
        CARGO_BUILD_TARGET = muslTarget;
        CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";

        # Tell pkg-config to look in musl package paths
        PKG_CONFIG_PATH = lib.makeSearchPath "lib/pkgconfig" [
          muslPkgs.openssl.dev
          muslPkgs.sqlite.dev
        ];

        # Point the linker to the musl cross-compiler
        CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER =
          "${muslPkgs.stdenv.cc}/bin/${muslPkgs.stdenv.cc.targetPrefix}cc";

        # For C dependencies like ring that use cc-rs
        CC_x86_64_unknown_linux_musl =
          "${muslPkgs.stdenv.cc}/bin/${muslPkgs.stdenv.cc.targetPrefix}cc";
        AR_x86_64_unknown_linux_musl =
          "${muslPkgs.stdenv.cc.bintools}/bin/${muslPkgs.stdenv.cc.targetPrefix}ar";
      };

    in
    {
      # Override the existing mls_validation_service derivation with musl cross-compilation settings.
      # The base derivation comes from rust-project.crates, which uses the toolchain defined in
      # rust.nix (already includes the musl target via mkToolchain).
      packages.musl-mls_validation_service =
        config.rust-project.crates.mls_validation_service.crane.outputs.drv.crate.overrideAttrs
          (old: muslEnv // {
            # Replace host buildInputs with musl-compiled versions
            buildInputs = with muslPkgs; [
              openssl
              sqlite
            ];

            # Override install phase to copy from musl target directory
            installPhaseCommand = ''
              mkdir -p $out/bin
              cp target/${muslTarget}/release/mls-validation-service $out/bin/
            '';
          });
    };
}
