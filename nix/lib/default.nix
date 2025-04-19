# nix/lib/default.nix
{ lib, ... }:
{
  flake.lib = {
    filesets = import ./filesets.nix;
  };
}

