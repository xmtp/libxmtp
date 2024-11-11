# Utility functions for cross-compiling
rec {
  # eachSystem [system] (system: ...)
  #
  # Returns an attrset with a key for every system in the given array, with
  # the key's value being the result of calling the callback with that key.
  eachSystem = supportedSystems: callback: builtins.foldl'
    (overall: system: overall // { ${system} = callback system; })
    {}
    supportedSystems;

  # eachCrossSystem [system] (buildSystem: hostSystem: ...)
  #
  # Returns an attrset with a key "$buildSystem.cross-$hostSystem" for
  # every combination of the elements of the array of system strings. The
  # value of the attrs will be the result of calling the callback with each
  # combination.
  #
  # There will also be keys "$system.default", which are aliases of
  # "$system.cross-$system" for every system.
  #
  eachCrossSystem = { buildSystem, supportedSystems, mkDerivationFor }:
    builtins.foldl'
        (allCross: hostSystem: allCross // { # the function being applied to `supportedSystems`
          "cross-${hostSystem}" = mkDerivationFor buildSystem hostSystem;
        })
        { default = mkDerivationFor buildSystem buildSystem; }
        supportedSystems;
}

# cross-iphone64-simulator = callback
