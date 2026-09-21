{
  stdenv,
  darwin,
  xmtp-pnpm,
  nodejs_26,
  pkg-config,
  lib,
  mkShell,
}:

mkShell {
  meta.description = "Node and agent SDK development environment";
  name = "xmtp-js-node environment";
  nativeBuildInputs = [ pkg-config ];
  buildInputs = [
    xmtp-pnpm
    nodejs_26
  ]
  ++ lib.optionals stdenv.isDarwin [ darwin.cctools ];
}
