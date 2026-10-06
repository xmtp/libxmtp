{ ... }:
{
  perSystem =
    { pkgs, lib, ... }:
    let
      xmtp = pkgs.xmtp;
      host = pkgs.stdenv.hostPlatform.rust.rustcTarget;
      sources = pkgs.callPackage ./lib/sdk-sources.nix { };
      targets = [
        "aarch64-apple-ios"
        "aarch64-apple-ios-sim"
      ];
      rust = (xmtp.craneLib.overrideScope (_: _: { stdenv = pkgs.stdenvNoCC; })).overrideToolchain (
        p: xmtp.mkToolchain p ([ host ] ++ targets) [ ]
      );
      ios =
        target:
        let
          command = ''
            ${xmtp.iosEnv.envSetup target}
            cargo build --release --locked -p xmtp_legacy_migration --lib --target ${target}
          '';
          options = {
            OPENSSL_NO_VENDOR = "0";
            OPENSSL_STATIC = "1";
            CARGO_BUILD_TARGET = target;
            __noChroot = true;
            buildPhaseCargoCommand = command;
          };
        in
        rust.buildPackage (
          xmtp.base.commonArgs
          // options
          // {
            pname = "xmtp-migration-${target}";
            version = xmtp.mkVersion rust;
            src = sources.legacyMigration rust;
            cargoArtifacts = xmtp.base.mkCargoArtifacts rust false options;
            doNotPostBuildInstallCargoBinaries = true;
            installPhaseCommand = ''
              mkdir -p $out/lib
              cp target/${target}/release/libxmtp_legacy_migration.a $out/lib/
            '';
          }
        );
    in
    {
      packages = {
        xmtp-migration-native = pkgs.callPackage ./package/xmtp-sdk-native.nix {
          crateName = "xmtp_legacy_migration";
        };
      }
      // lib.optionalAttrs pkgs.stdenv.isDarwin {
        xmtp-migration-ios-device = ios "aarch64-apple-ios";
        xmtp-migration-ios-simulator = ios "aarch64-apple-ios-sim";
      };
    };
}
