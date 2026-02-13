# Node.js NAPI-RS bindings: per-target .node files + JS/TS generation.
# Follows the same crane two-phase pattern as android.nix / ios.nix.
{
  lib,
  xmtp,
  nodejs,
  cacert,
  ...
}:
let
  inherit (xmtp) craneLib nodeEnv mobile;
  inherit (nodeEnv) targetToNapi;

  rust-toolchain = xmtp.mkToolchain nodeEnv.nodeTargets [ ];
  rust = craneLib.overrideToolchain (p: rust-toolchain);
  version = mobile.mkVersion rust;

  bindingsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = xmtp.filesets.forCrate ./../../bindings/node;
  };

  commonArgs = mobile.commonArgs // {
    cargoExtraArgs = "-p bindings_node";
    nativeBuildInputs = mobile.commonArgs.nativeBuildInputs ++ [ nodejs ];
  };

  # Per-target args shared between deps and build phases.
  mkTargetArgs =
    target:
    commonArgs
    // (nodeEnv.crossEnvFor target)
    // {
      CARGO_BUILD_TARGET = target;
      cargoExtraArgs = "--target ${target} -p bindings_node";
      nativeBuildInputs = commonArgs.nativeBuildInputs ++ (nodeEnv.crossPkgsFor target);
    };

  # Phase 1: build dependencies (cached separately per target)
  buildTargetDeps =
    target:
    rust.buildDepsOnly (
      mkTargetArgs target
      // {
        pname = "bindings-node-deps-${target}";
      }
    );

  # Phase 2: build the .node file using cached deps
  buildTarget =
    target:
    let
      napiName = targetToNapi.${target};
      libExt = if lib.hasInfix "apple" target then "dylib" else "so";
    in
    rust.buildPackage (
      mkTargetArgs target
      // {
        inherit version;
        pname = "bindings-node-${napiName}";
        src = bindingsFileset;
        cargoArtifacts = buildTargetDeps target;
        doNotPostBuildInstallCargoBinaries = true;
        installPhaseCommand = ''
          mkdir -p $out
          cp target/${target}/release/libbindings_node.${libExt} \
             $out/bindings_node.${napiName}.node
        '';
      }
    );

  # JS/TS generation: builds on host, runs napi CLI for index.js + index.d.ts.
  # Uses __noChroot for yarn install (network access). Must run on macOS in CI
  # since Linux enforces sandbox=true.
  jsBindings =
    let
      hostTarget = nodeEnv.hostTarget;
    in
    rust.buildPackage (
      commonArgs
      // {
        inherit version;
        pname = "bindings-node-js";
        src = bindingsFileset;
        cargoArtifacts = buildTargetDeps hostTarget;
        CARGO_BUILD_TARGET = hostTarget;
        cargoExtraArgs = "--target ${hostTarget} -p bindings_node";
        __noChroot = true;
        doNotPostBuildInstallCargoBinaries = true;
        installPhaseCommand = ''
          cd bindings/node
          export HOME=$TMPDIR
          export SSL_CERT_FILE=${cacert}/etc/ssl/certs/ca-bundle.crt
          export NODE_EXTRA_CA_CERTS=${cacert}/etc/ssl/certs/ca-bundle.crt
          node .yarn/releases/yarn-*.cjs install --immutable
          node node_modules/.bin/napi build --platform --release --esm \
            --target ${hostTarget}
          mkdir -p $out
          cp index.js $out/
          cp index.d.ts $out/
        '';
      }
    );

  mkNode = targetList: {
    targets = lib.genAttrs targetList buildTarget;
    inherit jsBindings;
  };

in
{
  inherit mkNode buildTarget jsBindings;
  inherit (mkNode nodeEnv.nodeTargets) targets;
}
