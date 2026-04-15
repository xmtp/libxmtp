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

  isGnu = stdenv.hostPlatform.isLinux && !stdenv.hostPlatform.isMusl;

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
  targetGlibcVersion = if isGnu then "2.27" else null;

  specialArgs =
    lib.optionalAttrs stdenv.hostPlatform.isMusl {
      # Use -crt-static to allow cdylib output on musl targets.
      RUSTFLAGS = "-C target-feature=-crt-static";
    }
    // lib.optionalAttrs isGnu {
      # overwrite build target for glibc
      CARGO_BUILD_TARGET = "${stdenv.hostPlatform.rust.rustcTarget}.${targetGlibcVersion}";
    };

  commonArgs =
    xmtp.base.commonArgs
    // {
      inherit version;
    }
    // specialArgs;

  cargoArtifacts = xmtp.base.mkCargoArtifacts rust test (
    specialArgs
    // lib.optionalAttrs isGnu {
      # override everything for glibc compatibility
      preBuild = "export HOME=$TMPDIR";
      nativeBuildInputs = xmtp.base.commonArgs.nativeBuildInputs ++ [ cargo-zigbuild ];
      buildPhaseCargoCommand = "cargo zigbuild ${maybeTestFeature} --profile $CARGO_PROFILE --locked";
    }
  );

in
rust.napiBuild (
  commonArgs
  // {
    inherit src cargoArtifacts;
    SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
    NODE_EXTRA_CA_CERTS = "${cacert}/etc/ssl/certs/ca-bundle.crt";
    napiExtraArgs = "-p bindings_node ${maybeTestFeature} --package-json-path ${src}/bindings/node/package.json";
    pname = "bindings-node-js";
    doInstallCargoArtifacts = false;
    napiGenerateJs = withJs;
    zigBuild = isGnu;
  }
  // lib.optionalAttrs stdenv.hostPlatform.isMusl {
    # remove nix specific rpaths for compatibility with musl dynamic linker
    postFixup = ''
      patchelf --remove-rpath $out/dist/bindings_node.*.node
    '';
  }
  // lib.optionalAttrs stdenv.hostPlatform.isDarwin {
    # sigtool provides `codesign` on PATH for the postFixup re-sign.
    # See https://github.com/xmtp/libxmtp/issues/3513.
    nativeBuildInputs = commonArgs.nativeBuildInputs ++ [
      darwin.autoSignDarwinBinariesHook
      darwin.sigtool
    ];
    postFixup = ''
      NODE_LIB=$(echo $out/dist/bindings_node.*.node)

      # Rewrite the dylib's own install name (LC_ID_DYLIB) so consumers
      # resolve it relative to the .node file, not the Nix build path.
      install_name_tool -id "@loader_path/$(basename $NODE_LIB)" "$NODE_LIB"

      # Rewrite every /nix/store/.../libiconv.<ver>.dylib load reference
      # to the macOS system copy. Using otool -L output as the source of
      # truth is drift-proof — we rewrite whatever the linker actually
      # recorded, not whatever Nix evaluation resolves darwin.libiconv to.
      # Cross-compile splicing in mkCrossPkgs was causing the two to
      # diverge, silently defeating a hardcoded `install_name_tool -change`.
      # See https://github.com/xmtp/libxmtp/issues/3516.
      # NR > 1 skips otool -L's header line (the file's own id).
      otool -L "$NODE_LIB" \
        | awk 'NR > 1 && $1 ~ /^\/nix\/store\/.*\/libiconv(\.[0-9]+)*\.dylib$/ { print $1 }' \
        | while read -r old; do
          install_name_tool -change "$old" "/usr/lib/$(basename "$old")" "$NODE_LIB"
        done

      # install_name_tool invalidates the ad-hoc signature applied by
      # darwin.autoSignDarwinBinariesHook; re-sign so the .node loads under
      # Gatekeeper on end-user macOS hosts.
      codesign --force --sign - "$NODE_LIB"

      # Assert no /nix/store references remain — guards against silent
      # no-ops in the rewrites above and catches the 1.10.0 regression.
      # See https://github.com/xmtp/libxmtp/issues/3479.
      # NR > 1 skips otool -L's header line (the file's own /nix/store path).
      remaining=$(otool -L "$NODE_LIB" | awk 'NR > 1 && $1 ~ /^\/nix\/store\// { print $1 }')
      if [ -n "$remaining" ]; then
        echo "error: $NODE_LIB still references /nix/store after postFixup:" >&2
        echo "$remaining" >&2
        exit 1
      fi
    '';
  }
)
