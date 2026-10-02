# The pinned UBRN Node addon for one SDK platform. JavaScript stays host-built.
{
  xmtp,
  lib,
  stdenv,
  pkg-config,
  perl,
  cargo-zigbuild,
  ubrnSrc,
  runtimeRevision,
}:
let
  target = stdenv.hostPlatform.rust.rustcTarget;
  nodeTarget = xmtp.toNapiTarget target;
  rust = xmtp.craneLib.overrideToolchain (p: xmtp.mkToolchain p [ target ] [ ]);
  isGnu = stdenv.hostPlatform.isLinux && !stdenv.hostPlatform.isMusl;
  buildTarget = target + lib.optionalString isGnu ".2.27";
  args = {
    pname = "xmtp-sdk-node-runtime-${nodeTarget}";
    version = runtimeRevision;
    src = ubrnSrc;
    cargoLock = ubrnSrc + /Cargo.lock;
    cargoExtraArgs = "--locked -p uniffi-runtime-napi --lib --target ${target}";
    CARGO_BUILD_TARGET = buildTarget;
    buildPhaseCargoCommand = "cargo ${
      if isGnu then "zigbuild" else "build"
    } --release --locked -p uniffi-runtime-napi --lib --target ${buildTarget}";
    CARGO_BUILD_JOBS = 2;
    nativeBuildInputs = [
      pkg-config
      perl
    ]
    ++ lib.optionals isGnu [ cargo-zigbuild ];
    doCheck = false;
    doInstallCargoArtifacts = false;
    doNotPostBuildInstallCargoBinaries = true;
    strictDeps = !stdenv.buildPlatform.isDarwin;
  }
  // lib.optionalAttrs stdenv.hostPlatform.isDarwin { MACOSX_DEPLOYMENT_TARGET = "11.0"; }
  // lib.optionalAttrs stdenv.hostPlatform.isMusl {
    RUSTFLAGS = "-C target-feature=-crt-static";
  };
  binary = "libuniffi_runtime_napi.${if stdenv.hostPlatform.isDarwin then "dylib" else "so"}";
  addon = "uniffi-runtime-napi.${nodeTarget}.node";
in
rust.buildPackage (
  args
  // {
    installPhaseCommand = ''
      mkdir -p $out
      cp target/${target}/release/${binary} $out/${addon}
      cat > $out/runtime-provenance.json <<'JSON'
      ${builtins.toJSON {
        schema = 1;
        revision = runtimeRevision;
        inherit target addon;
      }}
      JSON
    '';
  }
)
