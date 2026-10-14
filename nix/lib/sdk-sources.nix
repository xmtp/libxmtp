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
  # Swift and TypeScript trees.
  runtimeInputs = {
    swift = [ (root + /apps/xmtp_sdk_bindgen/runtime/swift) ];
    kotlin = [
      (root + /apps/xmtp_sdk_bindgen/runtime/kotlin)
      (root + /apps/xmtp_sdk_bindgen/runtime/android)
    ];
    typescript = [
      (root + /apps/xmtp_sdk_bindgen/runtime/ts)
      (root + /apps/xmtp_sdk_bindgen/typescript.toml)
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
        ++ runtimeInputs.${if lib.hasPrefix "typescript-" language then "typescript" else language}
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
  # Each identity is a digest that Nix computes at evaluation: the store path
  # of a closure source plus the files that every compile reads. It names no
  # store path in a product, and every system and build route records the
  # same value (`sdk-artifacts.py` reads it with `nix eval`).
  identity =
    inputs:
    builtins.hashString "sha256" (
      builtins.toJSON [
        (builtins.unsafeDiscardStringContext "${inputs}")
        (builtins.hashFile "sha256" (root + /Cargo.lock))
        (builtins.hashFile "sha256" (root + /.cargo/config.toml))
        (builtins.hashFile "sha256" (root + /rust-toolchain.toml))
      ]
    );
  provenance = {
    source = identity sdkInputs;
    generator = identity bindgenInputs;
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
