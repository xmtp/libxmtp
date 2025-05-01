{ cargo-cache
, cargo-machete
, cargo-features-manager
, cargo-bloat
, cargo-mutants
, cargo-deny
, cargo-nextest
, cargo-udeps
, cargo-generate
, extraInputs ? { }
, mkShell
, ...
}: (mkShell
  {
    buildInputs = [
      cargo-cache
      cargo-machete
      cargo-features-manager
      cargo-bloat
      cargo-mutants
      cargo-deny
      cargo-nextest
      cargo-udeps
      cargo-generate
    ];
  } // extraInputs)
