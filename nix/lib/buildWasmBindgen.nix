{ lib
, rustPlatform
, nodejs_latest
, pkg-config
, openssl
, stdenv
, curl
,
}:

{ version ? src.version
, src
, cargoDeps
}:

rustPlatform.buildRustPackage {
  pname = "wasm-bindgen-cli";

  inherit version src cargoDeps;

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
