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
        projectRootFile = ".git/config";
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
        };
      };
    };
}
