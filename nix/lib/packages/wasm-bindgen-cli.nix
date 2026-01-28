{ rustPlatform, pkg-config, fetchCrate, curl, nodejs_latest, openssl, stdenv, lib }:
let
  src = fetchCrate {
    pname = "wasm-bindgen-cli";
    version = "0.2.108";
    hash = "sha256-UsuxILm1G6PkmVw0I/JF12CRltAfCJQFOaT4hFwvR8E=";
  };

  cargoDeps = rustPlatform.fetchCargoVendor {
    inherit src;
    inherit (src) pname version;
    hash = "sha256-iqQiWbsKlLBiJFeqIYiXo3cqxGLSjNM8SOWXGM9u43E=";
  };
in
rustPlatform.buildRustPackage {
  pname = "wasm-bindgen-cli";

  inherit src cargoDeps;
  inherit (src) version;

  nativeBuildInputs = [ pkg-config ];

  buildInputs = [
    openssl
  ]
  ++ lib.optionals stdenv.hostPlatform.isDarwin [
    curl
  ];

  nativeCheckInputs = [ nodejs_latest ];

  # tests require it to be ran in the wasm-bindgen monorepo
  doCheck = false;
}
