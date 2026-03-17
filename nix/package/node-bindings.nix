{
  xmtp,
  cacert,
  lib,
  stdenv,
  darwin,
  cargo-zigbuild,
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
  targetGlibcVersion = "2.27";

  isGnu = stdenv.hostPlatform.isLinux && !stdenv.hostPlatform.isMusl;

  # crossEnv (CC_ vars) will be auto-populated in stdenv b/c using crossSystem in nixpkgs
  commonArgs =
    xmtp.base.commonArgs
    // {
      inherit src version;
      # strictDeps breaks darwin build with ring
      CARGO_PROFILE = "release";
      # this needs to be set for cargoArtifacts to inherit properly
      # since napi implicitly sets --target.
      CARGO_BUILD_TARGET = stdenv.hostPlatform.rust.rustcTarget;
    }
    // lib.optionalAttrs stdenv.hostPlatform.isMusl {
      # Use -crt-static to allow cdylib output on musl targets.
      RUSTFLAGS = "-C target-feature=-crt-static";
    }
    // lib.optionalAttrs isGnu {
      nativeBuildInputs = xmtp.base.commonArgs.nativeBuildInputs ++ [ cargo-zigbuild ];
      # overwrite build target for linux
      CARGO_BUILD_TARGET = "${stdenv.hostPlatform.rust.rustcTarget}.${targetGlibcVersion}";
    };

  cargoArtifacts = rust.buildDepsOnly (
    commonArgs
    // {
      buildPhaseCargoCommand = "cargo build -p bindings_node ${maybeTestFeature} --profile $CARGO_PROFILE --locked";
    }
    // lib.optionalAttrs isGnu {
      preBuild = "export HOME=$TMPDIR";
      buildPhaseCargoCommand = "cargo zigbuild -p bindings_node ${maybeTestFeature} --profile $CARGO_PROFILE --locked";
    }
  );

in
rust.napiBuild (
  commonArgs
  // {
    inherit cargoArtifacts;
    SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
    NODE_EXTRA_CA_CERTS = "${cacert}/etc/ssl/certs/ca-bundle.crt";
    napiExtraArgs = "-p bindings_node ${maybeTestFeature} --package-json-path ${src}/bindings/node/package.json";
    pname = "bindings-node-js";
    napiGenerateJs = withJs;
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
      install_name_tool -change "${darwin.libiconv}/lib/libiconv.2.dylib" "/usr/lib/libiconv.2.dylib" "$NODE_LIB"
    '';
  }
)
