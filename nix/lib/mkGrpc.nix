{ curl
, buf
, protobuf
, protoc-gen-prost-crate
, protolint
, mkShell
, extraInputs ? { }
, ...
}: (mkShell
  {
    buildInputs = [
      curl
      buf
      protobuf
      protoc-gen-prost-crate
      protolint
    ];
  } // extraInputs)
