# Checks that primarily run in CI
{ inputs, ... }: {
  perSystem = { pkgs, ... }:
    let
      craneLibPkgs = inputs.crane.mkLib pkgs;
      craneLib = craneLibPkgs.overrideToolchain (pkgs.xmtp.mkToolchain [ "wasm32-unknown-unknown" ] [ ]);
      nativeArtifacts = pkgs.callPackage pkgs.xmtp.mkWorkspace { inherit craneLib; };
      inherit (nativeArtifacts) cargoArtifacts commonArgs;

    in
    {
      checks = {
        workspace-clippy = craneLib.cargoClippy (commonArgs // {
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "--all-targets -- --deny warnings";
        });
        # Run tests with cargo-nextest
        # Consider setting `doCheck = false` on other crate derivations
        # if you do not want the tests to run twice
        workspace-nextest = craneLib.cargoNextest
          (commonArgs // {
            inherit cargoArtifacts;
            doCheck = true;
            partitions = 1;
            partitionType = "count";
            cargoNextestPartitionsExtraArgs = "--profile ci -E 'kind(lib) and deps(xmtp_mls)'";
          });
      };
    };
}
