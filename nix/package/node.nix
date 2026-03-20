# Node.js NAPI-RS bindings: per-target .node files + JS/TS generation.
# Follows the same crane two-phase pattern as android.nix / ios.nix.
{
  lib,
  xmtp,
  nodejs,
  cacert,
  darwin,
  cargo-zigbuild,
  ...
}:
let
  inherit (xmtp) craneLib nodeEnv mobile;
  inherit (nodeEnv) targetToNapi;

  rust-toolchain = xmtp.mkToolchain nodeEnv.nodeTargets [ ];
  rust = craneLib.overrideToolchain (p: rust-toolchain);
  version = xmtp.mkVersion rust;

  bindingsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = xmtp.filesets.forCrate ./../../bindings/node;
  };

  commonArgs = mobile.commonArgs // {
    cargoExtraArgs = "-p bindings_node";
    nativeBuildInputs = mobile.commonArgs.nativeBuildInputs ++ [ nodejs ];
  };

  isGnu = target: lib.hasInfix "gnu" target;

  # Per-target args shared between deps and build phases.
  mkTargetArgs =
    target:
    let
      # For GNU targets, cargo-zigbuild needs the glibc version suffix
      zigTarget =
        if isGnu target
        then "${target}.${nodeEnv.gnuGlibcVersion}"
        else target;
    in
    commonArgs
    // (nodeEnv.crossEnvFor target)
    // {
      CARGO_BUILD_TARGET = target;
      cargoExtraArgs = "--target ${zigTarget} -p bindings_node";
      nativeBuildInputs =
        commonArgs.nativeBuildInputs
        ++ (nodeEnv.crossPkgsFor target)
        ++ lib.optionals (isGnu target) [ cargo-zigbuild ];
    }
    // lib.optionalAttrs (isGnu target) {
      # Use cargo-zigbuild instead of cargo for GNU targets.
      # cargo-zigbuild uses zig as the linker to target a specific glibc version.
      cargoBuildCommand = "cargo zigbuild --profile release";
      cargoCheckCommand = "cargo zigbuild --profile release";
      # cargo-zigbuild caches zig downloads under $HOME/Library/Caches (macOS) or
      # $XDG_CACHE_HOME (Linux). In the Nix sandbox HOME=/homeless-shelter is read-only,
      # so we redirect the cache to $TMPDIR which is always writable.
      preBuild = "export HOME=$TMPDIR";
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
        nativeBuildInputs =
          (mkTargetArgs target).nativeBuildInputs
          ++ lib.optionals (lib.hasInfix "apple" target) [ darwin.cctools ];
        installPhaseCommand = ''
          mkdir -p $out
          cp target/${target}/release/libbindings_node.${libExt} \
             $out/bindings_node.${napiName}.node
        ''
        + lib.optionalString (lib.hasInfix "apple" target) ''
          # Rewrite Nix store rpaths to standard macOS system paths
          for nixlib in $(otool -L $out/bindings_node.${napiName}.node | grep /nix/store | awk '{print $1}'); do
            basename=$(basename "$nixlib")
            install_name_tool -change "$nixlib" "/usr/lib/$basename" \
              $out/bindings_node.${napiName}.node
          done
          # Re-sign after modification (install_name_tool invalidates ad-hoc signatures)
          codesign -s - $out/bindings_node.${napiName}.node || true
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

          # Force the generated loader to always use musl binaries on Linux.
          # napi-rs generates an isMusl() check that routes glibc hosts to GNU
          # targets, but we only ship musl builds (statically linked, work everywhere).
          grep -q 'isMusl()' index.js || \
            (echo "ERROR: napi-rs no longer generates isMusl() in index.js — loader patch needs updating" && exit 1)
          sed -i 's/if (isMusl())/if (true)/g' index.js

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
