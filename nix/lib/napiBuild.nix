# custom https://crane.dev `cargo napi` command
# https://crane.dev/custom_cargo_commands.html
# Does not generate ESM Js glue by default
{
  napi-rs-cli,
  cargo-zigbuild,
  stdenv,
  mkCargoDerivation,
  lib,
}:

{
  cargoArtifacts,
  napiExtraArgs ? "", # Arguments that are generally useful default
  napiGenerateJs ? false, # do not generate JS Glue
  CARGO_PROFILE ? "release",
  zigBuild ? false,
  ...
}@origArgs:
let
  # Clean the original arguments for good hygiene (i.e. so the flags specific
  # to this helper don't pollute the environment variables of the derivation)
  args = removeAttrs origArgs [
    "napiExtraArgs"
    "CARGO_PROFILE"
    "napiGenerateJs"
    "zigBuild"
  ];
  napiJsArgs = if napiGenerateJs then "--esm" else "--no-js";
  useZigBuild = if zigBuild then "-x" else "";
  hostTarget = stdenv.hostPlatform.rust.rustcTarget;
in
mkCargoDerivation (
  args
  // {
    # Additional overrides we want to explicitly set in this helper
    # Require the caller to specify cargoArtifacts we can use
    inherit cargoArtifacts;
    pnameSuffix = "-napi";
    preBuild = "export HOME=$TMPDIR";
    # Set the cargo command we will use and pass through the flags
    buildPhaseCargoCommand =
      args.buildPhaseCargoCommand or ''
        mkdir -p $out
        mkdir $out/dist

        # cargo zigbuild outputs to target/<triple>/ (without glibc version suffix)
        # but napi-rs-cli looks for artifacts at target/$CARGO_BUILD_TARGET/ which
        # may include a glibc suffix like ".2.27". Create a symlink so napi can find it.
        # this is a workarounc for https://github.com/napi-rs/napi-rs/issues/3176
        # and can be removed once 3176 is fixed.
        if [[ "''${CARGO_BUILD_TARGET:-}" == "${hostTarget}."* && ! -e "target/$CARGO_BUILD_TARGET" ]]; then
          mkdir -p target
          ln -sfn "${hostTarget}" "target/$CARGO_BUILD_TARGET"
        fi

        napi build --target-dir target --output-dir $out/dist \
          --platform --profile ${CARGO_PROFILE} \
          ${napiExtraArgs} ${napiJsArgs} ${useZigBuild} \
          -- --locked
      '';

    postFixup =
      args.postFixup or ''
        # strip glibc version suffix from output filenames (e.g. .linux-x64-gnu.2.27.node → .linux-x64-gnu.node)
        for f in $out/dist/*.node; do
          stripped=$(echo "$f" | sed -E 's/\.[0-9]+\.[0-9]+\.node$/.node/')
          if [ "$f" != "$stripped" ]; then
            mv "$f" "$stripped"
          fi
        done
      '';

    installPhaseCommand = args.installPhaseCommand or "true";
    doInstallCargoArtifacts = false;

    nativeBuildInputs =
      (args.nativeBuildInputs or [ ])
      ++ [
        napi-rs-cli
      ]
      ++ lib.optionals zigBuild [ cargo-zigbuild ];
  }
)
