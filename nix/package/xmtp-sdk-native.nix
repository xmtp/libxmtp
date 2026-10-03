# The generated SDK native library. Keep the old binding derivations separate.
{
  lib,
  xmtp,
  stdenv,
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
  special =
    lib.optionalAttrs android { buildInputs = [ ]; }
    // lib.optionalAttrs stdenv.hostPlatform.isMusl { RUSTFLAGS = "-C target-feature=-crt-static"; };
  command = "cargo build --release --locked -p xmtp_sdk --lib --target ${target}";
in
rust.buildPackage (
  xmtp.base.commonArgs
  // special
  // {
    pname = "xmtp-sdk-native-${target}";
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
      cp target/${target}/release/libxmtp_sdk.a $out/lib/
      cp target/${target}/release/libxmtp_sdk.${
        if stdenv.hostPlatform.isDarwin then "dylib" else "so"
      } $out/lib/
    '';
  }
)
