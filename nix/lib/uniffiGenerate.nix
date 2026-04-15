# custom https://crane.dev `uniffi-bindgen` command
{
  ffi-uniffi-bindgen,
  stdenv,
  lib,
}:

{
  bindgenExtraArgs ? "",
  dylibPath,
  language,
  ...
}@origArgs:
assert lib.assertOneOf "language" language [
  "swift"
  "kotlin"
];
let
  # Clean the original arguments for good hygiene (i.e. so the flags specific
  # to this helper don't pollute the environment variables of the derivation)
  args = removeAttrs origArgs [
    "bindgenExtraArgs"
    "ndkExtraArgs"
    "dylibPath"
    "language"
  ];

in
stdenv.mkDerivation (
  args
  // rec {
    # Additional overrides we want to explicitly set in this helper
    # Require the caller to specify cargoArtifacts we can use
    pname = "uniffi-bindgen";
    dontUnpack = true;
    # Set the cargo command we will use and pass through the flags
    buildPhase =
      args.buildPhaseCargoCommand or ''
        mkdir -p $out/${language}

        ffi-uniffi-bindgen generate \
        --library ${dylibPath} \
        --out-dir $out/${language} \
        --language ${language} \
        ${bindgenExtraArgs}
      '';

    installPhase = ''
      runHook preInstall
      ${installPhaseCommand}
      runHook postInstall
    '';

    installPhaseCommand = args.installPhaseCommand or "true";

    nativeBuildInputs = (args.nativeBuildInputs or [ ]) ++ [
      ffi-uniffi-bindgen
    ];
  }
)
