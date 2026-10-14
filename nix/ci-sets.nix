# Warm sets: the outputs that .github/workflows/post-merge.yml builds, pushes to the
# trusted Cachix cache `xmtp`, and pins on every push to trunk.
#
# `ciSets.<system>.dev` holds what developers and CI jobs use;
# `ciSets.<system>.release` holds what release jobs use. Each set is a link
# farm, so one store path holds the runtime closure of every member, and
# `passthru.members` lists the members for the closure check of post-merge.yml.
#
# A dev shell enters as its `inputDerivation`, whose runtime closure holds
# every shell input. Add a member only when a job, a release, or a developer
# builds that output from trunk.
{ lib, withSystem, ... }:
let
  # Every dev shell of the system, as `shell-<name>`.
  shells =
    self':
    lib.mapAttrs' (
      name: shell: lib.nameValuePair "shell-${name}" shell.inputDerivation
    ) self'.devShells;

  mkSet =
    pkgs: name: members:
    (pkgs.linkFarm "ci-set-${name}" members).overrideAttrs {
      passthru = { inherit members; };
    };

  sets = {
    x86_64-linux =
      { pkgs, self', ... }:
      let
        p = self'.packages;
      in
      {
        dev = mkSet pkgs "dev" (
          shells self'
          // {
            inherit (p)
              xmtp-sdk-libs
              android-sdk-libs-fast
              xmtp-sdk-generated-node
              xmtp-sdk-generated-browser
              xmtp-sdk-generated-kotlin
              xmtp-backend
              backend-image-x86_64-unknown-linux-musl
              ;
          }
        );
        # The arm64 Node pairs are cross outputs of this system. Release
        # builds them on arm64 runners until it takes these outputs.
        release = mkSet pkgs "release" {
          inherit (p)
            xmtp-sdk-node-linux-x64-gnu
            xmtp-sdk-node-linux-x64-musl
            xmtp-sdk-node-linux-arm64-gnu
            xmtp-sdk-node-linux-arm64-musl
            android-sdk-libs
            ;
        };
      };
    aarch64-darwin =
      { pkgs, self', ... }:
      let
        p = self'.packages;
      in
      {
        dev = mkSet pkgs "dev" (
          shells self'
          // {
            inherit (p)
              android-sdk-libs-fast
              ios-xcframeworks-fast
              xmtp-sdk-generated-node
              xmtp-sdk-generated-browser
              backend-ci
              backend-image-aarch64-unknown-linux-musl
              ;
          }
        );
        release = mkSet pkgs "release" {
          inherit (p)
            ios-xcframeworks
            xmtp-sdk-node-darwin-arm64
            ;
        };
      };
  };
in
{
  flake.ciSets = lib.mapAttrs (system: set: withSystem system set) sets;
}
