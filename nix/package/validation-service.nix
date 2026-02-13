# MLS Validation Service: cross-compiled Linux binary + Docker image.
# Follows the same crane two-phase pattern as node.nix.
{
  lib,
  xmtp,
  cacert,
  dockerTools,
  pkgsCross,
  stdenv,
  ...
}:
let
  inherit (xmtp) craneLib mobile;

  targets = [
    "x86_64-unknown-linux-gnu"
    "aarch64-unknown-linux-gnu"
  ];

  rust-toolchain = xmtp.mkToolchain targets [ ];
  rust = craneLib.overrideToolchain (p: rust-toolchain);
  version = mobile.mkVersion rust;

  serviceFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = xmtp.filesets.forCrate ./../../apps/mls_validation_service;
  };

  # Cross-compilation configuration per target.
  # aarch64-linux always cross-compiles. x86_64-linux is native on x86_64-linux,
  # cross-compiled on aarch64-darwin.
  crossConfig = {
    "aarch64-unknown-linux-gnu" = {
      cross = pkgsCross.aarch64-multiplatform;
      dockerArch = "arm64";
    };
    "x86_64-unknown-linux-gnu" = {
      cross = if stdenv.hostPlatform.system == "aarch64-darwin" then pkgsCross.gnu64 else null; # native on x86_64-linux
      dockerArch = "amd64";
    };
  };

  # Common build arguments shared between all targets.
  # buildInputs use host packages — the project vendors openssl+sqlcipher
  # (bundled-sqlcipher-vendored-openssl) and uses rustls for TLS, so cross
  # builds compile these from source using the cross CC. This matches node.nix.
  commonArgs = mobile.commonArgs // {
    cargoExtraArgs = "--package mls_validation_service --features test-utils";
  };

  # Per-target args with cross-compilation environment.
  # Follows node.nix pattern: only override CC/linker env vars and nativeBuildInputs
  # for cross targets; buildInputs stay as host packages (vendored deps handle the rest).
  mkTargetArgs =
    target:
    let
      config = crossConfig.${target};
      cross = config.cross;
      isCross = cross != null;
      cc = if isCross then cross.stdenv.cc else null;
      targetUpper = builtins.replaceStrings [ "-" ] [ "_" ] (lib.toUpper target);
    in
    commonArgs
    // {
      CARGO_BUILD_TARGET = target;
      cargoExtraArgs = "--target ${target} --package mls_validation_service --features test-utils";
      nativeBuildInputs = commonArgs.nativeBuildInputs ++ (if isCross then [ cc ] else [ ]);
    }
    // (
      if isCross then
        {
          "CC_${builtins.replaceStrings [ "-" ] [ "_" ] target}" = "${cc.targetPrefix}cc";
          "CARGO_TARGET_${targetUpper}_LINKER" = "${cc.targetPrefix}cc";
        }
      else
        { }
    );

  # Phase 1: build dependencies (cached separately per target).
  buildTargetDeps =
    target:
    rust.buildDepsOnly (
      mkTargetArgs target
      // {
        pname = "validation-service-deps-${target}";
      }
    );

  # Phase 2: build the binary using cached deps.
  buildBinary =
    target:
    rust.buildPackage (
      mkTargetArgs target
      // {
        inherit version;
        pname = "mls-validation-service-${target}";
        src = serviceFileset;
        cargoArtifacts = buildTargetDeps target;
        doNotPostBuildInstallCargoBinaries = true;
        installPhaseCommand = ''
          mkdir -p $out/bin
          cp target/${target}/release/mls-validation-service $out/bin/
        '';
      }
    );

  # Docker image via buildLayeredImage.
  buildImage =
    target:
    let
      binary = buildBinary target;
      config = crossConfig.${target};
    in
    dockerTools.buildLayeredImage {
      name = "ghcr.io/xmtp/mls-validation-service";
      tag = "main";
      architecture = config.dockerArch;
      contents = [
        binary
        cacert
      ];
      config = {
        Env = [
          "RUST_LOG=info"
          "SSL_CERT_FILE=${cacert}/etc/ssl/certs/ca-bundle.crt"
        ];
        Entrypoint = [ "${binary}/bin/mls-validation-service" ];
        ExposedPorts = {
          "50051/tcp" = { };
          "50052/tcp" = { };
        };
      };
    };

  # Map host system to Docker VM architecture for fast local builds.
  # aarch64-darwin Docker runs arm64 Linux VMs, x86_64-linux runs amd64 natively.
  hostDockerTarget =
    if stdenv.hostPlatform.system == "aarch64-darwin" then
      "aarch64-unknown-linux-gnu"
    else
      "x86_64-unknown-linux-gnu";

in
{
  inherit
    buildImage
    buildBinary
    hostDockerTarget
    ;
}
