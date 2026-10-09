# Messenger verification ledger

The approved app plan uses P1–P35 and V1–V9. These are plan identifiers, not SDK
spec identifiers. Existing SDK tests remain SDK proofs. App proofs must call
the production app boundary. A scripted reducer result cannot prove a native
transfer, metadata commit, or persisted SDK state.

This ledger records separate branch proofs. Combined verification waits for
final UI and push integration. A listed test is not a passing proof.
Fill each evidence cell with the source commit, command, result, and artifact
before the final app gate is ready. Label every manual result as manual.

| Requirement | App proof and required observation | Current evidence |
| --- | --- | --- |
| P1 Current exports | V1. Public `SDKClient` and records; no Rust export, schema, raw client, or app SQL addition | A package/build proof recorded; final source audit pending |
| P2 Shared UI | V1. Compile actual commonMain Compose screens and Android host under strict locks | A build proof recorded; combined app compilation pending |
| P3 Navy design | V8. Screen actions, 48 dp controls, light tokens; manual design, text scale, TalkBack, keyboard, empty/error cases | B216304 records real screens and reaction controls at 200% on a narrow display; TalkBack and manual checks for all screens remain pending |
| P4 Backend identity | V2. Same inbox on reopen; two isolated backend profiles; migration keeps old account files | B combined lifecycle proof pending |
| P5 Stale completion | V2/V3/V7/V8. Old session/screen completion changes no UI, marker, draft, field, or notification | Combined races and production guard red controls pending |
| P6 One message store | V1/V2. Preferences contain only small refs; secrets encrypted; no message mirror | Final persistence and backup audit pending |
| P7 Sign out | V2/V8. Signed-out state saved before failed unregister; retained wallet/database; cleared credential; delayed push suppressed | B/E combined proof pending |
| P8 Local reset | V2/V6. Live native delete and cold STOPPING cleanup; crash before/after database phase; IO failure retains reset; other profiles/files stay | B/C combined reset and grant proof pending |
| P9 Direct collection | V3. One default reader, sequential callback, cancellation/reopen; no asynchronous boundary or routine sync | B combined direct collector proof pending |
| P10 Projection rebuild | V3/V7. Lagged/event/foreground/reopen rereads current loaded values and preserves anchor where retained | Combined refresh and lost-event proof pending |
| P11 Consent | V3/V4/V8. Allowed/Unknown tabs, Allow/Block, fresh consent, denied row/notification removed | Combined consent action proof pending |
| P12 Local unread | V3. Exact incoming Published insertedAt.ns filters, same-time limit, stale newest-edge marker blocked | B real count and marker-race proof pending |
| P13 Timestamp buckets | V3. Independent raw counts, 80 tied rows, conversion-short 51-row page, 501 tie stop, no skipped valid row | B unit and broken/restored proof pending final audit |
| P14 Scroll restore | V3. Retained key/offset, deleted anchor, three-cache eviction, process loss, visible best-effort fallback | B real viewport/restore proof pending |
| P15 Bounded work | V3/V9. Four reads, first 50 handles, visible plus next 50 row reads, 500 published rows, three caches, 50-row overlay | B limits plus actual fixed-device performance pending |
| P16 Chat actions | V4. Real text/reply/reaction readback; animated five-choice inline reaction popover and full picker; normal composer reply first line and X that keeps draft text | B216304 records native reaction picker readback, reply quotes and X draft retention; action origin repair and final integration remain pending |
| P17 Stored retries | V4/V5. Accepted ID survives recovery; retry publishes that ID once; no typed resend | B/C native accepted-ID recovery pending |
| P18 Unknown send | V2/V5. Interrupted QUEUEING lacks ID; review/discard; no automatic queue; old screen queue completion fenced | B/C process and queue-outcome proof pending |
| P19 Removal/expiry | V4. Loaded content disappears or shows supported deleted placeholder after real delete and expiry | B device expiry and refresh proof pending |
| P20 Groups | V4. Create presets, name/description, members/roles, disappearing, failed intermediate preset, accurate PendingRemove | B native group edge cases pending |
| P21 File staging | V5. Missing/lying provider length, 64 KiB chunks, effective ceiling, identical names, private source cleanup | C production stager proof pending |
| P22 Upload first | V5. Failed/retried upload, queue after Complete only, discard active upload, no stale-screen queue | C real backend/object store proof pending |
| P23 Draft recovery | V5. Encrypted full descriptor; Complete reopen with pending(remote); short-age expiry; orphan; accepted ID takes precedence | C restart/pending/expiry proof pending |
| P24 Verified files | V5/V6. Digest verification, corrupt/missing object errors, sampled local preview, external receiver only gets one file, revoke/reset denies read | C real transfer and receiver APK proof pending |
| P25 Field discovery | V7. Committed descriptors by componentId; changed label keeps identity; unknown type has no read/write; offered missing field explained | D mapper and real catalogue proof pending |
| P26 Group values | V7. String/Bytes/map/set commits through app; delta keeps unrelated entries; limits and denied policy enforced | D two-client exact value/byte readback pending |
| P27 Own fields | V7. One changed-only updateUserData; own refs only; empty differs from Clear; group/DM values isolated | D two-client own/sibling readback pending |
| P28 Field refresh | V7. Events and failed/type-changed/policy-changed saves reread; actual error retained; stale completion rejected | D production controller red controls pending |
| P29 Optional Firebase | V8. Unconfigured strict build, transport Off, no registration or permission request; configured build also compiles | E a6 records configured/Off compilation; native Off/preflight proof and final integration remain pending |
| P30 Generic push | V8. Parse ULong, current known group/installation admission, mute/consent/token policy, dedupe, late A push after B sign-in, generic content | E a6 records host policy and restore controls; native cases remain pending; real background delivery needs a developer FCM project |
| P31 App gates | V1/V9. Current SDK host/package/consumer/platform gates plus actual launch and app tests in scoped emulator; no release publication | Final recipes, CI, and combined gate runs pending |
| P32 Failure detection | V1–V9. Each new test has a plausible broken production run and restored pass at a recorded source commit | Host performance result gate has broken/restored records; final per-test ledger pending |
| P33 Performance | V9. Exact 1000/100000 workload, one 50000 transcript, five warmups, 30 measured runs, all query/heap/retention limits | Test APK compiled; eight host validator/control tests pass; real Linux fixed-device run pending |
| P34 Credential visibility | V2/V8. Current server auth configuration shows the credential field only for its URL; stale capability results cannot show or hide the current field | B216304 reads the actual server with authentication disabled; scripted required-authentication and stale-URL branches have a matched control; final integration remains pending |
| P35 Attachment availability | V5/V8. SDK and current server support determine attachments; Start has no attachment checkbox or network switch | B/C final UI restack and screenshot proof pending |

## Recorded branch evidence

These results are from the separate branch reports. They do not prove the final
combined tree. The parent must verify the final restack and current source.

| Branch | Recorded checks | Remaining proof |
| --- | --- | --- |
| A shared build | Strict app/shared/SDK assembly, consumers and selected platform checks recorded in PR A | Combined final gates |
| B UI and recovery at 216304af4c | 30 native and 18 host tests pass; matched recovery and UX controls fail; restored gates pass; reaction controls and screenshots at 200% on a narrow display are observed | Action origin repair, final restack, review, TalkBack and manual checks for all screens |
| C attachments | 12 native cases pass, including actual SDK download/card update and external UID grants; stale-card control fails at the Verified UI assertion; grant and copy-ceiling controls fail; restoration passes | Single-filename visual cleanup has fast build/host/lint proof only; new screenshot and final UI restack |
| D metadata | 30 host tests pass, including ten metadata tests; twelve matching host controls fail; 60 configuration fixtures pass; real catalogue backend startup and cleanup pass | Native two-client metadata writes and exact value/byte readback |
| E sealed fast proof at a6ea784f0c | 38 host tests and strict configured/Off compilation pass; synthetic public resources prove build only | Reservation repair, eight native cases and controls, final restack and real Firebase delivery |
| F performance | Matched immutable bindings and actual SDK/app/shared/test APK assembly pass; eight host validator/control tests pass | Exact fixed Linux run, intended cache failure and restored measurement on the same dataset; observed seed time must set the final CI timeout |

The execution reports retain source checkpoints, commands, XML and failure
patches. B evidence is `pr-b-ux-all-device-green.log` and
`pr-b-ux-native-green-results`/`pr-b-ux-host-green-results` XML at B216304af4c.
C evidence is `pr-c-native-rebased-restored-12.log` and XML,
`pr-c-control-card-state-results.xml`, and the isolated grant/ceiling controls.
D evidence is `pr-d-checkpoint-gates.log`, `pr-d-host-controls-final.log`, and
`pr-d-lint-config-final.log`. A later compile does not replace a missing device
result. B's observed 200% proof covers the narrow reaction controls. It does
not establish accessibility for all screens. Real FCM, TalkBack and V9 budgets
remain unverified.

## Required final commands

Run from the repository root, inside Nix:

```sh
dev/nix-shell 'just android assemble'
dev/nix-shell 'just android check'
dev/nix-shell 'just android lint'
dev/nix-shell 'just android test'
dev/nix-shell 'just android check-consumers'
dev/nix-shell 'just android example-test'
dev/nix-shell 'just android test-integration'
dev/nix-shell 'just android example-test-integration'
dev/nix-shell 'just android example-performance-check'
dev/nix-shell 'just android example-performance'
dev/nix-shell 'just lint-config'
dev/nix-shell 'just spec-check'
```

Keep the selected API 23 and all-ABI package jobs. The app instrumentation
recipe must own its emulator, fixture process, forwarding, and teardown. Add
the fixed performance job without replacing an existing library gate.

## Evidence that cannot be inferred

- Real FCM background receipt and tap need a developer Firebase project and a
  backend FCM channel. A synthetic configured build proves compilation only.
- Performance needs the fixed Linux emulator or a named fixed physical device
  accepted before measurement. Darwin ARM functional tests prove neither.
- B records narrow reaction controls at 200% text scale. Text scale on all
  screens, TalkBack, keyboard and visual checks still need observed results.
- Test source and a green helper test do not prove the full V2–V8 scenario.
- Removing a dead app test requires its behavior and surviving callers to be
  checked. Keep `ExampleStorageTest`. Keep the SDK package, logging, lifecycle,
  attachment, and metadata tests.
