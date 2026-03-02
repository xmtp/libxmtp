# Defaults for every crate in the workspace
# Only autowire crate and clippy by default.
# Individual crates can override this in rust.nix
# "Auto Wiring" means that the `rust-flake` module
# autoscans the repository for rust crates,
# and automatically uses `crane.dev` to build them.
# then "wires" the output of the build to
# nix outputs. EX: Wire the "clippy" output to the flake output under "checks".
# this allows running nix check .#crate-clippy
# By default disabled to avoid redoing what other
# GH Actions are already accomplishing.
{ inputs, lib, ... }:
let
  src = ./..;
  cargoToml = fromTOML (builtins.readFile (src + "/Cargo.toml"));
  inherit (import "${inputs.rust-flake}/nix/crate-parser" { inherit lib; }) findCrates;

  cratePaths = findCrates cargoToml.workspace.members src;
  getCrateName =
    path:
    let
      crateCargoToml = fromTOML (builtins.readFile "${src}/${path}/Cargo.toml");
    in
    crateCargoToml.package.name;
  allCrateNames = map getCrateName cratePaths;
  # Map from crate name to crate path
  crateNameToPath = lib.listToAttrs (
    map (path: {
      name = getCrateName path;
      value = path;
    }) cratePaths
  );
in
{
  perSystem =
    { pkgs, lib, ... }:
    let
      src = ./..;
      workspaceFileset =
        crate:
        lib.fileset.toSource {
          root = ./..;
          fileset = pkgs.xmtp.filesets.forCrate crate;
        };
    in
    {
      # for each crate set the crate output and clippy output
      # also ensure fileset is correct
      rust-project.crates = lib.genAttrs allCrateNames (crate: {
        crane.args.src = lib.mkDefault (workspaceFileset (src + "/${crateNameToPath.${crate}}"));
        # Use mkDefault so rust.nix can override with normal priority
        autoWire = lib.mkDefault [ ];
      });
    };
}
