{
  fenix,
  lib,
  zstd,
  sqlite,
  perl,
  craneLib,
  xmtp,
  openssl,
  pkg-config,
}:
let

  # Pinned Rust Version
  rust-toolchain = fenix.combine [
    fenix.stable.cargo
    fenix.stable.rustc
  ];
  rust = craneLib.overrideToolchain (p: rust-toolchain);

  workspaceFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = (xmtp.filesets { inherit lib craneLib; }).workspace;
  };
  commonArgs = {
    src = rust.cleanCargoSource ./../..;
    strictDeps = true;
    nativeBuildInputs = [
      pkg-config
      perl
    ];

    buildInputs = [
      zstd
      sqlite
      openssl
    ];
    doCheck = false;
  };

  cargoArtifacts = rust.buildDepsOnly commonArgs;

in
rust.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    src = workspaceFileset;
    pname = "xmtp-debug";
    version = "1.9.2";
    cargoExtraArgs = "-p xdbg";
  }
)
