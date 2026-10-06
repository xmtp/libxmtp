{ lib, xmtp }:
let
  root = ./../..;
  inherit (lib.fileset) unions fileFilter toSource;
  cargoSources = xmtp.craneLib.fileset.commonCargoSources;
  workspaceManifest = builtins.fromTOML (builtins.readFile (root + /Cargo.toml));
  workspaceDependencies = workspaceManifest.workspace.dependencies;
  patches = workspaceManifest.patch."crates-io";
  dependencyTables =
    manifest:
    [
      (manifest.dependencies or { })
      (manifest.build-dependencies or { })
    ]
    ++ lib.concatMap (target: [
      (target.dependencies or { })
      (target.build-dependencies or { })
    ]) (builtins.attrValues (manifest.target or { }));
  localDependencies =
    path: manifest:
    lib.concatMap (
      table:
      lib.concatLists (
        lib.mapAttrsToList (
          name: value:
          let
            inherited = builtins.isAttrs value && (value.workspace or false);
            dependency = if inherited then workspaceDependencies.${name} else value;
            package = if builtins.isAttrs dependency then dependency.package or name else name;
            patch = patches.${package} or { };
            base = if inherited then root else path;
          in
          if builtins.isAttrs dependency && dependency ? path then
            [ { key = toString (base + "/${dependency.path}"); } ]
          else if patch ? path then
            [ { key = toString (root + "/${patch.path}"); } ]
          else
            [ ]
        ) table
      )
    ) (dependencyTables manifest);
  asPath = key: root + (lib.removePrefix (toString root) key);
  closure =
    package:
    map (entry: asPath entry.key) (
      builtins.genericClosure {
        startSet = [ { key = toString package; } ];
        operator =
          entry:
          localDependencies (asPath entry.key) (
            builtins.fromTOML (builtins.readFile "${entry.key}/Cargo.toml")
          );
      }
    );
  sdkCrates = closure (root + /crates/xmtp_sdk);
  bindgenCrates = closure (root + /apps/xmtp_sdk_bindgen);
  workspaceSource = toSource {
    inherit root;
    fileset = xmtp.filesets.workspace;
  };
  embedded = [
    (root + /proto)
    (root + /crates/xmtp_db/migrations)
    (root + /crates/xmtp_attachments/src/address-registry.txt)
    (root + /crates/xmtp_id/src/scw_verifier/chain_urls_default.json)
    (root + /crates/xmtp_id/src/scw_verifier/signature_validation.hex)
  ];
  restored =
    crates: extra:
    toSource {
      inherit root;
      fileset = unions (
        [ (root + /Cargo.toml) ]
        ++ map cargoSources crates
        ++ map (crate: fileFilter (file: lib.hasSuffix ".sql" file.name) crate) crates
        ++ extra
      );
    };
  sdkInputs = restored sdkCrates embedded;
  bindgenInputs = restored bindgenCrates [
    (root + /apps/xmtp_sdk_bindgen/templates)
    (root + /apps/xmtp_sdk_bindgen/src/swift_event_fixture.swift)
    (root + /apps/xmtp_sdk_bindgen/runtime/ts/bridge/worker/host.ts)
    (root + /crates/xmtp_sdk/src/client/event_conformance.rs)
    (root + /crates/xmtp_sdk/src/foreign_conformance.rs)
  ];
  mkCompileSource =
    rust: inputs:
    rust.mkDummySrc {
      src = workspaceSource;
      extraDummyScript = ''
        cp --recursive --remove-destination ${inputs}/. $out/
      '';
    };
  generationSource =
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
  sdk = rust: mkCompileSource rust sdkInputs;
  bindgen = rust: mkCompileSource rust bindgenInputs;
}
