# SDK finish baseline

Checked on 2026-09-30 for PR A (#4334), Tasks 1 and 2. The approved plan is
[TiFDtuzx3U19olnv](https://plan.ref.tools/TiFDtuzx3U19olnv).

## Start gates

The lane A baseline is `2d66de1107214e084d266c3856537e9bf6378a69`. It contains all four lane A gate PRs.
The original full gate is also met at this commit. PRs A and B are still open.
Decision 16 requires their ready heads and a simulated merge before C starts.

| PR | State | Merged at (UTC) | Merge commit | In baseline |
| --- | --- | --- | --- | --- |
| #4276 | MERGED | 2026-09-29T22:33:12Z | `c18950a085f7e46666685e3b3f105c5e3715e6db` | yes |
| #4278 | MERGED | 2026-09-30T00:12:44Z | `d007c5cc9bff4aa8e989fe626025e85673d98cd2` | yes |
| #4308 | MERGED | 2026-09-30T00:12:46Z | `d05c255b5fd677a697be0eaa9c21bcce121f8b1b` | yes |
| #4316 | MERGED | 2026-09-30T05:35:31Z | `c3e93a8168de1984cd03e2fb8c4fd97ed760c67b` | yes |
| #4293 | MERGED | 2026-09-30T04:21:09Z | `2ef478c70ea42cde48700c85856a098e53e2ae9f` | yes |
| #4313 | MERGED | 2026-09-29T21:52:16Z | `47745c10e6cd7af332b4f4e559268314fd24f92b` | yes |
| #4314 | MERGED | 2026-09-29T21:52:17Z | `918e14224158f35dfa7732e68164e28ec54e15f9` | yes |
| #4317 | MERGED | 2026-09-29T21:52:18Z | `038646478a7cdecdbc427a9b5ee141544a864d67` | yes |
| #4319 | MERGED | 2026-09-29T22:24:00Z | `9f81cd2e72ec945d15ba6dc39e912c109e8ba6d5` | yes |
| #4315 | MERGED | 2026-09-29T22:41:25Z | `452a7baddebcfb17a0106d50015b8370632157df` | yes |
| #4322 | MERGED | 2026-09-30T05:24:28Z | `77a68d47a8681461daa86da2eb708fdc0ae8782c` | yes |
| #4247 | MERGED | 2026-09-29 07:31:34 | `992a524d7a9b7a9bdcee3552adec9f5339d33b7f` | yes |
| #4248 | MERGED | 2026-09-29 07:31:36 | `68e1f4ca1ea221c0cb52c4656ea0677b8ec329f3` | yes |
| #4252 | MERGED | 2026-09-29 07:31:37 | `6e729ec89cd3902abd40f39fe52b254c7d374f24` | yes |
| #4254 | MERGED | 2026-09-29 07:31:39 | `02cad5255691a5b86183ad25e14cb4d5ab700919` | yes |
| #4258 | MERGED | 2026-09-29 07:31:40 | `a747984d4ea4eafb70e1b1441a812ee065cafc90` | yes |

The older brief names #4249 for the attachment stack. GitHub reports no such PR.
That reference cannot prove a gate. The actual gates are #4247, #4248, #4252, #4254 and #4258. Their merged heads are ancestors of this baseline.

PR #4293 merged all five F4 slices. The merged generator emits public roots.
No F4 slice moved to this task under the absorption rule.

## Later base integration

Lane A later merges `4a07869955370c7f6e3b161f50e058b0adbc5c13`, which contains
PR #4343. It preserves mobile configuration error kinds and re-checks selected
missing groups. The start-gate evidence above remains tied to the original
baseline. The new base needs current-head CI after stack submission.

Lane A then merges `d157751b85946ee22dc5d327f2864c0b0dad0a6e`. This adds
PR #4342 archive and legacy decode changes and PR #4346 documentation changes.
The merged archive requirements and gap waivers stay intact. Metadata field
authority and conformance configuration admission still need current-source
checks. The original start-gate evidence does not change.

## Pins

| Source | Pin |
| --- | --- |
| Cargo workspace SDK version | `1.12.0-dev` |
| UniFFI | `0.32.2` |
| uniffi-bindgen-react-native fork | `330f9edbc3c4d6e6e948f6d2eb2724358668b79a` |
| Shipped node package | `6.2.0-dev` |
| Shipped browser package | `7.2.0-dev` |
| Shipped agent package | `2.3.0` |
| `sdks/ios/XMTP.podspec` | `4.12.0-dev` |
| `sdks/android/gradle.properties` | `4.12.0-dev` |

Cargo.toml, Cargo.lock, and nix/lib/packages/ubrn.nix hold the generator pins.
No package switches or publication occur in Tasks 1 and 2.

## Retained public surface

The temporary member inventory and its check are retired after the final
package switch. Do not add a second member database.
`conformance/public/` has SwiftPM, Android Context, Node, and browser consumers.

| Host | Supported root | Current reader exposure |
| --- | --- | --- |
| Swift | `XmtpSdk` runtime with `SDKClient` | Generated Group/Dm messageReader and raw next remain public. |
| Kotlin | `uniffi.xmtp_sdk` runtime with `SDKClient` | Generated Group/Dm messageReader and raw next remain public; stream Flow is public. |
| Node | Generated `typescript-napi/index.ts` | Public MessageReader.next and factories remain exported. |
| Browser | Generated `typescript-wasm/index.ts`, with pure helpers from `typescript-pure/index.ts` | Public MessageReader.next and factories remain exported. |

Task 5 owns the reader adapter boundary and compiler negative proof. Task 10
owns release installed package creation. The browser metadata worker fixture
uses its own MainSession for the test catalogue; it proves worker projection,
not ordinary installed Client.create. That installed proof remains in Task 10.

PR A adds the metadata records and eight async methods to Group and Dm.
ServerConfiguration adds applicationComponents. The pure synchronous
metadataFieldRef helper takes WellKnownMetadataField. Stock generation exposes
these records and methods on all four hosts. There is no handwritten host list.

Metadata types: MetadataFieldRef, MetadataFieldDescriptor, MetadataComponentType,
MetadataScalarType, MetadataKeyType, FieldValue, FieldKey, MetadataValue, MapEntry,
MetadataFieldValue, UserFieldValue, UserFieldUpdate, MapMutation, SetMutation,
ComponentMutation, ComponentPermissions, MetadataPolicy, MetadataBasePolicy,
ApplicationComponentDefinition, and WellKnownMetadataField.

Metadata methods: metadataFields, metadataField, metadataValue, metadataValues,
mapValue, userData, updateUserData, and updateMetadataField. Field identity uses
componentId. Names are labels. Filters keep absent and empty distinct.

Metadata errors: UnknownField, NotUserField, DuplicateField, TypeMismatch,
UnsupportedType, TypeChanged, and PermissionDenied. Component errors keep the
component source mapping. They do not use display text for classification.

## Generator controls

The following hand-maintained controls are current. Later lanes must update the
owning source when they add a public member.

- `bridge/mod.rs` IMMUTABLE_PROPERTIES permits only these synchronous snapshots:

```rust
const IMMUTABLE_PROPERTIES: &[&str] = &[
    "Client.app_version",
    "Client.archives",
    "Client.client_key",
    "Client.conversations",
    "Client.diagnostics",
    "Client.identity",
    "Client.inbox_id",
    "Client.installation_id",
    "Client.installation_id_bytes",
    "Client.is_in_memory",
    "Client.libxmtp_version",
    "Client.options",
    "Client.preferences",
    "Client.server_configuration",
    "Client.storage",
    "Client.storage_path",
    "Dm.added_by_inbox_id",
    "Dm.created_at",
    "Dm.creator_inbox_id",
    "Dm.id",
    "Dm.is_creator",
    "Dm.kind",
    "Dm.peer_inbox_id",
    "Dm.topic",
    "Group.added_by_inbox_id",
    "Group.created_at",
    "Group.creator_inbox_id",
    "Group.id",
    "Group.is_creator",
    "Group.kind",
    "Group.topic",
];
```

- `validate.rs` requires async Reader.end. It rejects close methods, record methods,
  wrong error types, raw message records, and unsupported synchronous foreign calls.
  LogSink.log is the existing synchronous exception.
- `check-parity-signatures.py` compares exact generated root declarations. It lists
  SDK-037 native exclusions, pure module exports, and platform members. The pure
  list includes metadataFieldRef; public platform-specific declarations have no skip.
- `check-public-names` rejects `_at_ns` names.

## Shared storage and later boundaries

Lane B replaces Path with Explicit { dbPath, attachmentsDir }. Default and
Directory use the core deployment/inbox data_dir layout. Explicit keeps the
given paths and permits offline reopen without a caller inbox ID. InMemory
keeps no inferred attachment directory. Host roots and labels stay with B.

## Proof state

V1 passed: live GitHub JSON and git ancestry establish the original lane and full
gates above. Raw JSON is in the takeover run under `proofs/a-start-gates.json` and
`proofs/4339-baseline.json` (actual attachment gate heads).
V2 retained public-consumer is pending on the takeover candidate. V3 host and
Rust checks are recorded in `proofs/4334.md` in that run. No historical green
head establishes a pass on the takeover candidate. Browser installed creation
and final release package proof remain Task 10. Waivers close only after every
required host proof passes (P31).
