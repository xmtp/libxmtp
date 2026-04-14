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
    nativeBuildInputs = commonArgs.nativeBuildInputs ++ [
      darwin.autoSignDarwinBinariesHook
    ];
    # Rewrite /nix/store dylib references to macOS system paths so the .node
    # file loads on hosts without Nix. We iterate `otool -L` output rather
    # than using a fixed `install_name_tool -change` against
    # `${darwin.libiconv}`, because nixpkgs drift (we follow nixos-unstable,
    # unpinned since #3482) or cross-compile splicing can make a hardcoded
    # path match silently a no-op. See
    # https://github.com/xmtp/libxmtp/issues/3479.
    postFixup = ''
      NODE_LIB=$(echo $out/dist/bindings_node.*.node)

      # Fix the dylib's own install name (LC_ID_DYLIB) — it embeds the
      # ephemeral Nix build path.
      install_name_tool -id "@loader_path/$(basename $NODE_LIB)" "$NODE_LIB"

      # Rewrite every /nix/store libiconv reference to the system libiconv,
      # which macOS resolves via the dyld shared cache. Narrow the rewrite
      # to libiconv (the only known Nix-provided dep with a /usr/lib
      # counterpart) rather than blanket-replacing all /nix/store paths;
      # any remaining /nix/store reference is caught by the assert below.
      otool -L "$NODE_LIB" \
        | awk '$1 ~ /^\/nix\/store\// && $1 ~ /\/libiconv\.[0-9.]+\.dylib$/ { print $1 }' \
        | while read -r nixlib; do
          install_name_tool -change "$nixlib" "/usr/lib/$(basename "$nixlib")" "$NODE_LIB"
        done

      # install_name_tool invalidates the ad-hoc signature applied by
      # autoSignDarwinBinariesHook, so re-sign after modification.
      codesign --force --sign - "$NODE_LIB"

      # Fail loudly if any /nix/store reference remains. Catches new deps
      # leaking Nix paths at build time rather than at consumer-report time.
      remaining=$(otool -L "$NODE_LIB" | awk '$1 ~ /^\/nix\/store\// { print $1 }')
      if [ -n "$remaining" ]; then
        echo "error: $NODE_LIB still references /nix/store after postFixup:" >&2
        echo "$remaining" >&2
        exit 1
      fi
    '';
  }
)
