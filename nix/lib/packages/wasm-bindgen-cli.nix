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
    version = "0.2.120";
    hash = "sha256-Dkkx8Bhfk+y/jEz9Fzwytmv2N3Gj/7ST+5MlPRzzetU=";
  };

  cargoDeps = rustPlatform.fetchCargoVendor {
    inherit src;
    inherit (src) pname version;
    hash = "sha256-5Zu/Sh9aBMxB+KGC1MHWJAQ8PuE40M6lsenkpFEwJ6A=";
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
  };
}
