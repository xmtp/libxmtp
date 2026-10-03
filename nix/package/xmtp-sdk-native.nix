# The generated SDK native library. Keep the old binding derivations separate.
{
  lib,
  xmtp,
  stdenv,
  python3,
  android ? false,
}:
let
  target = stdenv.hostPlatform.rust.rustcTarget;
  rust = xmtp.craneLib.overrideToolchain (p: xmtp.mkToolchain p [ target ] [ ]);
  root = ./../..;
  source = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      xmtp.filesets.workspace
      (lib.fileset.fileFilter (
        file:
        (
          lib.hasSuffix ".rs" file.name || lib.hasSuffix ".proto" file.name || lib.hasSuffix ".sql" file.name
        )
        || file.name == "Cargo.toml"
      ) (root + /crates))
      (lib.fileset.fileFilter (
        file:
        (
          lib.hasSuffix ".rs" file.name || lib.hasSuffix ".proto" file.name || lib.hasSuffix ".sql" file.name
        )
        || file.name == "Cargo.toml"
      ) (root + /apps))
      (lib.fileset.fileFilter (
        file:
        (
          lib.hasSuffix ".rs" file.name || lib.hasSuffix ".proto" file.name || lib.hasSuffix ".sql" file.name
        )
        || file.name == "Cargo.toml"
      ) (root + /bindings))
      (root + /flake.lock)
      (root + /rust-toolchain.toml)
      (root + /crates/xmtp_sdk)
      (root + /apps/xmtp_sdk_bindgen)
      (root + /crates/xmtp_configuration)
    ];
  };
  special = {
    OPENSSL_NO_VENDOR = "0";
    OPENSSL_STATIC = "1";
  }
  // lib.optionalAttrs stdenv.hostPlatform.isDarwin {
    MACOSX_DEPLOYMENT_TARGET = "11.0";
    # Darwin setup replaces this variable before the Cargo build.
    preBuild = "export MACOSX_DEPLOYMENT_TARGET=11.0";
  }
  // lib.optionalAttrs android { buildInputs = [ ]; }
  // lib.optionalAttrs stdenv.hostPlatform.isMusl { RUSTFLAGS = "-C target-feature=-crt-static"; };
  command = "cargo build --release --locked -p xmtp_sdk --lib --target ${target}";
in
rust.buildPackage (
  xmtp.base.commonArgs
  // special
  // {
    pname = "xmtp-sdk-native-${target}";
    nativeBuildInputs = xmtp.base.commonArgs.nativeBuildInputs ++ [ python3 ];
    version = xmtp.mkVersion rust;
    src = source;
    cargoArtifacts = xmtp.base.mkCargoArtifacts rust false (
      special
      // {
        CARGO_BUILD_TARGET = target;
        buildPhaseCargoCommand = command;
      }
    );
    CARGO_BUILD_TARGET = target;
    buildPhaseCargoCommand = command;
    doNotPostBuildInstallCargoBinaries = true;
    installPhaseCommand = ''
      mkdir -p $out/lib
      python3 - <<'PYTHON' > $out/native-provenance.json
      import importlib.util, json
      spec = importlib.util.spec_from_file_location("artifacts", "crates/xmtp_sdk/dev/sdk-artifacts.py")
      artifacts = importlib.util.module_from_spec(spec)
      spec.loader.exec_module(artifacts)
      print(json.dumps({"schema": 1, "source": artifacts.source_hash(), "generator": artifacts.source_hash(True), "target": "${target}", "profile": "release", "features": ""}))
      PYTHON
      cp target/${target}/release/libxmtp_sdk.a $out/lib/
      cp target/${target}/release/libxmtp_sdk.${
        if stdenv.hostPlatform.isDarwin then "dylib" else "so"
      } $out/lib/
    '';
  }
)
