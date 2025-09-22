{ rustPlatform, pkg-config, fetchCrate, curl, nodejs_latest, openssl, stdenv, lib }:
let
  src = fetchCrate {
    pname = "wasm-bindgen-cli";
    version = "0.2.101";
    hash = "sha256-txpbTzlrPSEktyT9kSpw4RXQoiSZHm9t3VxeRn//9JI=";
  };

  cargoDeps = rustPlatform.fetchCargoVendor {
    inherit src;
    inherit (src) pname version;
    hash = "sha256-J+F9SqTpH3T0MbvlNKVyKnMachgn8UXeoTF0Pk3Xtnc=";
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
