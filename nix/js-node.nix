{
  stdenv,
  darwin,
  corepack,
  nodejs_24,
  pkg-config,
  lib,
  mkShell,
}:

mkShell {
  meta.description = "Node and agent SDK development environment";
  name = "xmtp-js-node environment";
  nativeBuildInputs = [ pkg-config ];
  buildInputs = [
    corepack
    nodejs_24
  ]
  ++ lib.optionals stdenv.isDarwin [ darwin.cctools ];
}
