{ lib, xmtp }:
let
  root = ./../..;
  inherit (lib.fileset) unions toSource;
  workspaceSource = toSource {
    inherit root;
    fileset = xmtp.filesets.workspace;
  };
  # Compiler sources restore the selected local dependency graph over a dummy
  # workspace. The closure adds the data files that its packages compile in.
  sdkClosure = {
    roots = [ (root + /crates/xmtp_sdk) ];
  };
  bindgenClosure = {
    roots = [ (root + /apps/xmtp_sdk_bindgen) ];
    extra = [
      (root + /apps/xmtp_sdk_bindgen/templates)
      (root + /apps/xmtp_sdk_bindgen/src/swift_event_fixture.swift)
      (root + /apps/xmtp_sdk_bindgen/runtime/ts/bridge/worker/host.ts)
    ];
  };
  sdkInputs = xmtp.filesets.closureSource sdkClosure;
  bindgenInputs = xmtp.filesets.closureSource bindgenClosure;
  # Each render reads only its own runtime, so a Kotlin runtime edit keeps the
  # Swift and TypeScript renders.
  runtimeInputs = {
    swift = [ (root + /apps/xmtp_sdk_bindgen/runtime/swift) ];
    kotlin = [
      (root + /apps/xmtp_sdk_bindgen/runtime/kotlin)
      (root + /apps/xmtp_sdk_bindgen/runtime/android)
    ];
    # RN shares the public TypeScript projection but has its own host runtime.
    # Keep this subtree out of the sibling TypeScript render identity.
    react-native = runtimeInputs.typescript ++ [
      (lib.fileset.maybeMissing (root + /apps/xmtp_sdk_bindgen/runtime/react-native))
      (lib.fileset.maybeMissing (root + /apps/xmtp_sdk_bindgen/react-native.toml))
    ];
    typescript = [
      (root + /apps/xmtp_sdk_bindgen/runtime/ts)
      (root + /apps/xmtp_sdk_bindgen/typescript.toml)
      # The formatter configuration of generated TypeScript.
      (root + /apps/xmtp_sdk_bindgen/templates/bridge/oxfmt.json)
    ];
  };
  generationInputs =
    language:
    toSource {
      inherit root;
      fileset = unions (
        [
          (root + /apps/xmtp_sdk_bindgen/uniffi-global.toml)
          (root + /crates/xmtp_sdk/uniffi.toml)
        ]
        ++ runtimeInputs.${
          if language == "typescript-react-native" then
            "react-native"
          else if lib.hasPrefix "typescript-" language then
            "typescript"
          else
            language
        }
      );
    };
  # The stock Swift/Kotlin renderer still asks Cargo for workspace metadata.
  # Keep manifests and target stubs without adding real compilation sources.
  generationSource =
    language:
    xmtp.craneLib.mkDummySrc {
      src = workspaceSource;
      extraDummyScript = ''
        cp --recursive --remove-destination ${generationInputs language}/. $out/
      '';
    };
  # Each identity is a digest that Nix computes at evaluation: the store paths
  # of its sources plus the files that every compile reads. It names no store
  # path in a product, and every system and build route records the same value
  # (`sdk-artifacts.py` reads it with `nix eval`).
  identity =
    inputs:
    builtins.hashString "sha256" (
      builtins.toJSON (
        map (input: builtins.unsafeDiscardStringContext "${input}") inputs
        ++ [
          (builtins.hashFile "sha256" (root + /Cargo.lock))
          (builtins.hashFile "sha256" (root + /.cargo/config.toml))
          (builtins.hashFile "sha256" (root + /rust-toolchain.toml))
        ]
      )
    );
  provenance = {
    source = identity [ sdkInputs ];
    # The bindgen executable.
    generator = identity [ bindgenInputs ];
    # A generated tree is the bindgen's output over its own language's runtime
    # and configuration. Its receipt records this identity, so a runtime edit
    # changes only the trees of that language.
    render = lib.genAttrs [ "swift" "kotlin" "typescript" "react-native" ] (
      language:
      identity [
        bindgenInputs
        (generationInputs language)
      ]
    );
  };
in
{
  inherit
    provenance
    generationSource
    sdkInputs
    bindgenInputs
    ;
  sdk = rust: xmtp.filesets.mkClosureSource rust sdkClosure;
  bindgen = rust: xmtp.filesets.mkClosureSource rust bindgenClosure;
}
