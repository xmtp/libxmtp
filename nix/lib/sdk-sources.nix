{ lib, xmtp }:
let
  root = ./../..;
  inherit (lib.fileset) unions fileFilter toSource;
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
  generationInputs =
    language:
    toSource {
      inherit root;
      fileset = unions [
        (root + /apps/xmtp_sdk_bindgen/uniffi-global.toml)
        (root + /apps/xmtp_sdk_bindgen/typescript.toml)
        (root + /crates/xmtp_sdk/uniffi.toml)
        (
          if language == "swift" then
            root + /apps/xmtp_sdk_bindgen/runtime/swift
          else
            root + /apps/xmtp_sdk_bindgen/runtime
        )
      ];
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
  # Keep the complete source used by both existing identity producers.
  provenanceSource = toSource {
    inherit root;
    fileset = unions [
      xmtp.filesets.workspace
      (fileFilter (
        file:
        lib.hasSuffix ".rs" file.name
        || lib.hasSuffix ".proto" file.name
        || lib.hasSuffix ".sql" file.name
        || file.name == "Cargo.toml"
      ) (root + /crates))
      (fileFilter (
        file:
        lib.hasSuffix ".rs" file.name
        || lib.hasSuffix ".proto" file.name
        || lib.hasSuffix ".sql" file.name
        || file.name == "Cargo.toml"
      ) (root + /apps))
      (root + /flake.lock)
      (root + /rust-toolchain.toml)
      (root + /crates/xmtp_sdk)
      (root + /apps/xmtp_sdk_bindgen)
      (root + /crates/xmtp_configuration)
    ];
  };
in
{
  inherit
    provenanceSource
    generationSource
    sdkInputs
    bindgenInputs
    ;
  sdk = rust: xmtp.filesets.mkClosureSource rust sdkClosure;
  bindgen = rust: xmtp.filesets.mkClosureSource rust bindgenClosure;
}
