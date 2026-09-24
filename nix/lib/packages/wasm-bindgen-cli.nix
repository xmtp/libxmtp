{
  rustPlatform,
  pkg-config,
  fetchCrate,
  curl,
  nodejs_latest,
  openssl,
  stdenv,
  lib,
}:
let
  src = fetchCrate {
    pname = "wasm-bindgen-cli";
    version = "0.2.126";
    hash = "sha256-H6Is3fiZVxZCfOMWK5dWMSrtn50VGv0sfdnsT+cTtyk=";
  };

  cargoDeps = rustPlatform.fetchCargoVendor {
    inherit src;
    inherit (src) pname version;
    hash = "sha256-VucqkXbCi4qtQzY/HrXiDnbSURsagPsdNVMn1Tw3UiY=";
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
  meta = {
    description = "Custom maintained wasm-bindgen-cli package to match Cargo.toml";
    mainProgram = "wasm-bindgen";
  };
}
