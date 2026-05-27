{ inputs, ... }:
{
  perSystem =
    { pkgs, ... }:
    let
      rustfmt = (pkgs.fenix.fromManifestFile inputs.rust-manifest).rustfmt;
    in
    {
      treefmt = {
        flakeFormatter = true;
        flakeCheck = true;
        projectRootFile = "flake.nix";
        programs = {
          nixfmt.enable = true;
          rustfmt = {
            enable = true;
            package = rustfmt;
            excludes = [
              "crates/xmtp_proto/src/gen/*"
              "crates/xmtp-workspace-hack/*"
            ];
          };
          ruff-format.enable = true;
          ruff-check.enable = true;
          # Rule set lives in /.editorconfig so spotless and treefmt agree.
          ktlint.enable = true;
          taplo.enable = true;
          shellcheck = {
            enable = true;
            # Override defaults to drop *.envrc / *.envrc.*
            includes = [
              "*.sh"
              "*.bash"
            ];
            excludes = [
              "*.env"
              "**/Dockerfile"
            ];
          };
        };
        settings.formatter = {
          shellcheck.options = [
            "-e"
            "SC1091"
            "-e"
            "SC2046"
            "-e"
            "SC2086"
            "-e"
            "SC2016"
            "-e"
            "SC2164"
            "-e"
            "SC2181"
          ];
          # nicklockwood/swiftformat — not bundled in treefmt-nix's program
          # list (only apple/swift-format is, and that one is broken).
          # Settings live in .swiftformat at repo root.
          swiftformat = {
            command = "${pkgs.swiftformat}/bin/swiftformat";
            includes = [ "*.swift" ];
          };
        };
      };
    };
}
