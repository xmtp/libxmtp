{ otherShells ? [ ]
, mkShell
, hello
, extraInputs
, stdenv
}:
let
  # Evaluate the fn if its a function, otherwise leave it alone
  fnOrSet = x:
    if builtins.isFunction x then
      x { }
    else
      x;
  evaluatedShells = builtins.map (x: (fnOrSet x)) otherShells;
in
mkShell.override { inherit stdenv; } ({
  inputsFrom = [ hello ] ++ evaluatedShells;
} // extraInputs)
