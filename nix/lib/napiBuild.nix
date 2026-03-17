# custom https://crane.dev `cargo napi` command
# https://crane.dev/custom_cargo_commands.html
# Does not generate ESM Js glue by default
{
  napi-rs-cli,
  mkCargoDerivation,
}:

{
  cargoArtifacts,
  napiExtraArgs ? "", # Arguments that are generally useful default
  napiGenerateJs ? false, # do not generate JS Glue
  CARGO_PROFILE ? "release",
  ...
}@origArgs:
let
  # Clean the original arguments for good hygiene (i.e. so the flags specific
  # to this helper don't pollute the environment variables of the derivation)
  args = removeAttrs origArgs [
    "napiExtraArgs"
    "CARGO_PROFILE"
    "napiGenerateJs"
  ];
  napiJsArgs = if napiGenerateJs then "--esm" else "--no-js";
in
mkCargoDerivation (
  args
  // {
    # Additional overrides we want to explicitly set in this helper
    # Require the caller to specify cargoArtifacts we can use
    inherit cargoArtifacts;
    pnameSuffix = "-napi";
    # Set the cargo command we will use and pass through the flags
    buildPhaseCargoCommand =
      args.buildPhaseCargoCommand or ''
        mkdir -p $out
        mkdir $out/dist
        napi build --target-dir target --output-dir $out/dist \
          --platform --profile ${CARGO_PROFILE} \
          ${napiExtraArgs} ${napiJsArgs} \
          -- --locked
      '';
    installPhaseCommand = args.installPhaseCommand or "true";
    doInstallCargoArtifacts = false;

    # Append the `cargo-awesome` package to the nativeBuildInputs set by the
    # caller (or default to an empty list if none were set)
    nativeBuildInputs = (args.nativeBuildInputs or [ ]) ++ [ napi-rs-cli ];
  }
)
