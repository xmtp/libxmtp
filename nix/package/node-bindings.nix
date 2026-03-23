{
  xmtp,
  cacert,
  lib,
  stdenv,
  darwin,
  test ? false,
  withJs ? false,
}:
let
  inherit (xmtp) craneLib;
  inherit (lib.fileset) unions;
  inherit (craneLib.fileset) commonCargoSources;
  # p is important here, since crane splices packages according to host/build platform
  # so it must be used to create the right toolchain for the platform.
  rust-toolchain = p: xmtp.mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ ];

  # overrideToolchain accepts a function that accepts `p` a pkg set
  rust = craneLib.overrideToolchain rust-toolchain;
  root = ./../..;
  src = lib.fileset.toSource {
    inherit root;
    fileset = unions [
      xmtp.filesets.libraries
      (commonCargoSources (root + /bindings/node))
      (root + /bindings/node/package.json)
    ];
  };
  maybeTestFeature = if test then "--features test-utils" else "";
  version = xmtp.mkVersion rust;
  # crossEnv (CC_ vars) will be auto-populated in stdenv b/c using crossSystem in nixpkgs
  commonArgs =
    xmtp.base.commonArgs
    // {
      inherit version;
      CARGO_PROFILE = "release";
      CARGO_BUILD_TARGET = stdenv.hostPlatform.rust.rustcTarget;
    }
    // lib.optionalAttrs stdenv.hostPlatform.isMusl {
      # Use -crt-static to allow cdylib output on musl targets.
      RUSTFLAGS = "-C target-feature=-crt-static";
    };

  cargoArtifacts = xmtp.base.mkCargoArtifacts rust test;
in
rust.napiBuild (
  commonArgs
  // {
    inherit src cargoArtifacts;
    SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
    NODE_EXTRA_CA_CERTS = "${cacert}/etc/ssl/certs/ca-bundle.crt";
    napiExtraArgs = "-p bindings_node ${maybeTestFeature} --package-json-path ${src}/bindings/node/package.json";
    pname = "bindings-node-js";
    napiGenerateJs = withJs;
    INVALIDATE_CACHE = 1;
  }
  // lib.optionalAttrs stdenv.hostPlatform.isMusl {
    # remove nix specific rpaths for compatibility with musl dynamic linker
    postFixup = ''
      patchelf --remove-rpath $out/dist/bindings_node.*.node
    '';
  }
  // lib.optionalAttrs stdenv.hostPlatform.isDarwin {
    nativeBuildInputs = commonArgs.nativeBuildInputs ++ [
      darwin.autoSignDarwinBinariesHook
    ];
    postFixup = ''
      NODE_LIB=$(echo $out/dist/bindings_node.*.node)

      # Fix the dylib's own install name (LC_ID_DYLIB) — it embeds the ephemeral Nix build path
      install_name_tool -id "@loader_path/$(basename $NODE_LIB)" "$NODE_LIB"
    '';
  }
)
