{
  lib,
  writeShellScriptBin,
  bash,
  coreutils,
  python3,
  lsof,
  postgresql_18,
  versitygw,
  awscli2,
  grpc-health-probe,
  grpcurl,
  backend,
}:
writeShellScriptBin "backend-ci" (
  builtins.replaceStrings
    [ "@tools@" "@backend@" "@config@" "@policy@" "@cors@" ]
    [
      (lib.makeBinPath [
        bash
        coreutils
        python3
        lsof
        postgresql_18
        versitygw
        awscli2
        grpc-health-probe
        grpcurl
      ])
      "${backend}/bin/xmtp-backend"
      "${../../dev/backend/local-s3.toml}"
      "${../../dev/docker/s3/public-read.json}"
      "${../../dev/docker/s3/cors.json}"
    ]
    (builtins.readFile ../../dev/backend/ci)
)
