# Messenger verification ledger

The approved app plan uses P1–P35 and V1–V9. These are plan identifiers, not SDK
spec identifiers. Existing SDK tests remain SDK proofs. App proofs must call
the production app boundary. A scripted reducer result cannot prove a native
transfer, metadata commit, or persisted SDK state.

The current parent is `ada6699f3e9f32bdfc50464a0ed15f73a9e20d57` on E
`23696a2b37584d6975755d12f18aff5bbc140046`, D
`a2eb8b69b7faa89105973c93d8a4661a48bbab63`, C
`ed4f6ee535afaf57fbbb781935f02317ff2e369a` and B
`8c4929abe3ecf363b4c46f19a198c2b324480215`.
F's restored four-case app suite passes with 14 intended native controls. Final
scale reproof at `52e1d4aae4074492ffdf9ada98af0c33212859e9` passes on actual
320 dp/200% configuration. All nine final images are inspected. Settings is
captured before its modal; the post-Cancel dialog-title absence assertion passes.
The earlier restored action proof retains its exact source map. Linux V9, real
FCM and TalkBack remain unverified. C7006 later records restored28 and host27;
that newer parent still needs the root restack/source map.

This ledger separates current source checks from recorded branch runtime proof.
A listed test or compile is not a passing native result.
Fill each evidence cell with the source commit, command, result, and artifact
before the final app gate is ready. Label every manual result as manual.

| Requirement | App proof and required observation | Current evidence |
| --- | --- | --- |
| P1 Current exports | V1. Public `SDKClient` and records; no Rust export, schema, raw client, or app SQL addition | A package/build proof recorded; final source audit pending |
| P2 Shared UI | V1. Compile actual commonMain Compose screens and Android host under strict locks | Current combined strict app/shared/test APK compilation recorded; native and final CI gates remain pending |
| P3 Navy design | V8. Screen actions, 48 dp controls, light tokens; manual design, text scale, TalkBack, keyboard, empty/error cases | F restored4 checks all nine screens at 320 dp/200%, actual 48 dp touch targets, physical scrolling and exact caption layouts; 14 controls fail intended assertions; My fields is readable; settled Settings image and post-Cancel absence pass at F52e1; TalkBack remains pending |
| P4 Backend identity | V2. Same inbox on reopen; two isolated backend profiles; migration keeps old account files | Bf230 records profile/reopen/reset proof in its 38-case native suite; current combined native replay remains pending |
| P5 Stale completion | V2/V3/V7/V8. Old session/screen completion changes no UI, marker, draft, field, or notification | Bf230 records action/session/group admission controls; E1bac restore host controls pass; current E/F native replay remains pending |
| P6 One message store | V1/V2. Preferences contain only small refs; secrets encrypted; no message mirror | Final persistence and backup audit pending |
| P7 Sign out | V2/V8. Signed-out state saved before failed unregister; retained wallet/database; cleared credential; delayed push suppressed | B/E combined proof pending |
| P8 Local reset | V2/V6. Live native delete and cold STOPPING cleanup; crash before/after database phase; IO failure retains reset; other profiles/files stay | B/C combined reset and grant proof pending |
| P9 Direct collection | V3. One default reader, sequential callback, cancellation/reopen; no asynchronous boundary or routine sync | Bf230 records direct collection and ACK admission; E owner/teardown changes need current combined native replay |
| P10 Projection rebuild | V3/V7. Lagged/event/foreground/reopen rereads current loaded values and preserves anchor where retained | Bf230 and D1d5 record actual projection/event refresh; current combined native replay remains pending |
| P11 Consent | V3/V4/V8. Allowed/Unknown tabs, Allow/Block, fresh consent, denied row/notification removed | Combined consent action proof pending |
| P12 Local unread | V3. Exact incoming Published insertedAt.ns filters, same-time limit, stale newest-edge marker blocked | B real count and marker-race proof pending |
| P13 Timestamp buckets | V3. Independent raw counts, 80 tied rows, conversion-short 51-row page, 501 tie stop, no skipped valid row | B unit and broken/restored proof pending final audit |
| P14 Scroll restore | V3. Retained key/offset, deleted anchor, three-cache eviction, process loss, visible best-effort fallback | B real viewport/restore proof pending |
| P15 Bounded work | V3/V9. Four reads, first 50 handles, visible plus next 50 row reads, 500 published rows, three caches, 50-row overlay | B limits plus actual fixed-device performance pending |
| P16 Chat actions | V4. Real text/reply/reaction readback; animated five-choice inline reaction popover and full picker; normal composer reply first line and X that keeps draft text | Bf230 retains the B216304 reaction/reply proof and adds native admission/security/group controls; current E/F native replay remains pending |
| P17 Stored retries | V4/V5. Accepted ID survives recovery; retry publishes that ID once; no typed resend | Cd543 records actual stored-ID card retry without its secure descriptor and intended ViewChat-only control; current combined replay remains pending |
| P18 Unknown send | V2/V5. Interrupted QUEUEING lacks ID; review/discard; no automatic queue; old screen queue completion fenced | B/C process and queue-outcome proof pending |
| P19 Removal/expiry | V4. Loaded content disappears or shows supported deleted placeholder after real delete and expiry | B device expiry and refresh proof pending |
| P20 Groups | V4. Create presets, name/description, members/roles, disappearing, failed intermediate preset, accurate PendingRemove | F restored4 passes actual shared-settings edits, both presets, expiry/Off, member/admin addition/removal and real PendingRemove; nine matched SDK mutation controls fail intended assertions, then restoration passes |
| P21 File staging | V5. Missing/lying provider length, 64 KiB chunks, effective ceiling, identical names, private source cleanup | Cd543 restored20 records provider/copy/ceiling proof with matching controls; current F native replay remains pending |
| P22 Upload first | V5. Failed/retried upload, queue after Complete only, discard active upload, no stale-screen queue | Cd543 restored20 records actual upload/retry/discard, Complete admission and accepted-ID-only retry; new discard controls fail intended assertions; current F native replay remains pending |
| P23 Draft recovery | V5. Encrypted full descriptor; Complete reopen with pending(remote); short-age expiry; orphan; accepted ID takes precedence | Cd543 restored20 records coordinator reopen, pending expiry, Complete admission, failed-discard rollback and accepted/unknown preservation; literal OS process kill is not claimed; current F replay remains pending |
| P24 Verified files | V5/V6. Digest verification, corrupt/missing object errors, sampled local preview, external receiver only gets one file, revoke/reset denies read | Cd543 restored20 records verified card, external UID grants, revoke/reset and final one-filename image; current F native replay remains pending |
| P25 Field discovery | V7. Committed descriptors by componentId; changed label keeps identity; unknown type has no read/write; offered missing field explained | D1d5 records real controller/shared UI catalogue reads and unknown/type/policy host cases; current F native replay remains pending |
| P26 Group values | V7. String/Bytes/map/set commits through app; delta keeps unrelated entries; limits and denied policy enforced | D1d5 records actual peer readbacks and group mutations with matched native controls; current F native replay remains pending |
| P27 Own fields | V7. One changed-only updateUserData; own refs only; empty differs from Clear; group/DM values isolated | D1d5 records actual group/DM own and sibling values, dirty refs and draft preservation; current F replay remains pending |
| P28 Field refresh | V7. Events and failed/type-changed/policy-changed saves reread; actual error retained; stale completion rejected | D1d5 records actual error/refresh/admission/event/draft controls and restored native cases; current F replay remains pending |
| P29 Optional Firebase | V8. Unconfigured strict build, transport Off, no registration or permission request; configured build also compiles | Ebc972 passes both strict builds and app lint in this checkpoint; ten Off cases and 15 controls at E2318, plus actual configured denial/control/restoration at E90fc |
| P30 Generic push | V8. Parse ULong, current known group/installation admission, mute/consent/token policy, dedupe, late A push after B sign-in, generic content | E2318 records ten Off cases and 15 intended production-control failures; captured publisher proof does not establish real FCM or OS posting |
| P31 App gates | V1/V9. Current SDK host/package/consumer/platform gates plus actual launch and app tests in scoped emulator; no release publication | Final recipes, CI, and combined gate runs pending |
| P32 Failure detection | V1–V9. Each new test has a plausible broken production run and restored pass at a recorded source commit | F14 native controls each have one named assertion failure, zero errors/skips; restored4 passes; first fixture/harness failures are separate and uncredited; host performance broken/restored records remain |
| P33 Performance | V9. Exact 1000/100000 workload, one 50000 transcript, five warmups, 30 measured runs, all query/heap/retention limits | Test APK compiled; eight host validator/control checks and the generated-message wrapper regression pass; real Linux fixed-device run pending |
| P34 Credential visibility | V2/V8. Current server auth configuration shows the credential field only for its URL; stale capability results cannot show or hide the current field | B216304 reads the actual server with authentication disabled; scripted required-authentication and stale-URL branches have a matched control; final integration remains pending |
| P35 Attachment availability | V5/V8. SDK and current server support determine attachments; Start has no attachment checkbox or network switch | Cd543 records actual availability/reply and captured-token stale-support proof plus final verified card image; current F replay remains pending |

## Current combined source checks

The earlier full fast checkpoint at `2211ef223a5895433653924ec8d2adb58477d480`
passed both strict builds, host50 and app lint in each variant, performance8,
format, SDK lint, config72 and Markdown. Its source evidence is `pr-f-439-*`.
Later parent updates received the minimum changed strict compile and exact source
mapping, rather than an unchanged full-suite repeat. Current F source passes
strict Off app/shared/debug/release/test APK compilation. The final screenshot
order/dialog-absence correction passes narrow native scale reproof at F52e1.
No synthetic Firebase resources remain.

E's separate configured/Off publishers resolve the prior app lint failure.
The configured publisher checks permission before `notify`; the Off publisher
does not post. Both actual app lint variants pass. No suppression or Off permission
declaration was added. This does not prove device permission or notification behavior.
F restored4 and its 14 causal native controls are recorded below. They do not
prove a fixed Linux V9 result. The capture correction has separate narrow native
proof at F52e1.

## Recorded branch evidence

These results are from the separate branch reports. They do not prove the final
combined tree. The parent must verify the final restack and current source.

| Branch | Recorded checks | Remaining proof |
| --- | --- | --- |
| A shared build | Strict app/shared/SDK assembly, consumers and selected platform checks recorded in PR A | Combined final gates |
| B8c | Prior B1091 has 43 debug and two release native cases; B80183 has two harness cases; B8c has four restored repair cases, a final two-case refinement and host24 in each build | No fresh full 46-case suite; current F replay, Linux confirmation, TalkBack and manual checks for all screens |
| Ced4 | Earlier Cd543 restored20 and controls; later watcher/phase/orphan subset has mapped source/runtime proof; accepted-action wrapping has F native scale proof | C7006 later records cancellation/picker closure and restored28; root parent restack/source mapping remains |
| Da2eb | Metadata/immutable source/native/control and PostgreSQL signal proof remain in D report; F restored scale checks the new full-width status and wrapped own actions | Final stack review and platform CI |
| E23696 | Prior Off10/15 controls and real denied publisher proof; later scoped privacy/controller and actual VM navigation controls/restored proof are source-mapped in E report | Real Firebase delivery and granted OS post/tap; final stack CI |
| F current | Strict parent compile and restored4 pass; 14 intended native assertion failures, zero errors/skips; exact map for nine earlier group controls across D layout restack | Exact Linux V9/cache-red/restoration; observed seed progress must set timeout |

The execution reports retain exact source, command, XML and failure patches.
B evidence is `pr-b-round17-native-debug-final-results` and
`pr-b-round17-native-release-final-results` at
`1091b31162f5e75491e3faf7a466996709b233a9`, plus
`pr-b-round18-native-two-results` at B80183. The prior Linux run at B1091 had two
app failures; the Mac ARM64 harness result does not prove a new Linux pass.
B8c repair evidence is `pr-b-round19-native-<stage>-results` and
`pr-b-round19-host-restored-results`. Its four-case restored set and final two-case
owner/recovery set do not constitute a full 46-case native run.
The B216304 UX proof remains in `pr-b-ux-native-green-results` and its controls.
C runtime source is `a6756ac9a932890a2add7a787673ba290002bde5`.
`pr-c-harness-base-source-equivalence.json` records exact source-tree equality
with Cbd26; its range-diff records all 24 unchanged C patches. The queue/support
controls and `pr-c-final-b-card.png` remain historical runtime evidence.
D evidence is `pr-d-native-restored-results`, the eight `pr-d-native-red-*` paths,
and `pr-d-final-source-hashes.txt` at D1d5. E's earlier fast proof is `pr-e-d1-*`
on E1bac; it does not replace E's pending native repairs.
E2318 Off runtime is `pr-e-b8c-restored-native`; controls are
`pr-e-control-matrix-summary.json` and its per-case XML/logcat.
`pr-e-b8c-repaired-native` records the repaired cases and startup Retry.
The old fixture failures are superseded by these restored branch results.
C closure is `pr-c-round20-restored-20-final-results.xml` and the five
`pr-c-control-*` results. The earlier 19/20 run is not restoration evidence.
E publisher evidence is `pr-e-publisher-baseline`, `pr-e-publisher-runtime-red`
and `pr-e-publisher-runtime-restored`. Valid channels isolate actual permission
denial and publisher return behavior. No successful OS post or FCM is claimed.
Current F source checks use `pr-f-439-*` logs.
`pr-f-439-source-evidence.json` records the unchanged performance/group proof,
C attachment trees and the E90fc publisher fixture mapping. The scale fixture now has two real staged
files and one real uploaded optimistic accepted SDK ID. It checks staged Send
file/Discard and accepted Retry publication/View chat/Discard without clicking
Discard. The scale case also checks FD00–FD03 initial immutable inputs/actions.
The FieldUi canWrite branch uses immutable and componentPresent; actual tagged
inputs must exist for the unset catalogue fields. No fake FieldUi is used.
This is prepared proof only. `pr-f-439-base-delta.txt` records the inherited delta;
the ViewModel and performance workload are unchanged.
A compile does not replace
missing device proof. B's observed 200% proof covers reaction controls on a narrow
display. It does not prove all-screen accessibility. Real FCM, TalkBack and V9
budgets remain unverified.

## Prepared native app proof

`GroupSettingsInstrumentedTest` uses the real shared settings controls and the
ViewModel, then reads native SDK state. It covers group edits, both presets,
duration, member addition/removal, admin promotion/demotion and a real member's
PendingRemove UI. Direct SDK writes create fixture peers and the member group;
they are not the behavior under test. A completion observer reports the finished
app action, so an omitted mutation can fail its readback assertion directly.

`ScreenScaleInstrumentedTest` sets an actual 320 dp display and 200% font scale
before Activity launch. It visits all nine screens with a real catalogue session
and real SDK staged and accepted drafts. It uses physical swipes, checks scrolling progress,
48 dp action bounds and label clipping, and saves each screen. It restores the
previous display settings after Activity teardown. The action-bound case and full restored4 pass. Narrow scale reproof at F52e1
then passes the capture-before-modal and post-Cancel dialog absence correction.
All nine final images are inspected. TalkBack service proof is absent.

Both classes have restored4 native proof and 14 intended control failures. Nine
group controls omit actual SDK writes; scroll controls remove real user scrolling;
three caption controls clip Admins only, omit its layout action or return an empty
layout list. Each fresh XML has one assertion failure and zero errors/skips. The
first unplaced/scroll/keyboard/recovery fixture failures and the malformed control
compile are retained separately and are not coverage.

## Required final commands

Run from the repository root, inside Nix:

```sh
dev/nix-shell 'just android assemble'
dev/nix-shell 'just example-android check'
dev/nix-shell 'just example-android lint'
dev/nix-shell 'just android check'
dev/nix-shell 'just android lint'
dev/nix-shell 'just android test'
dev/nix-shell 'just android check-consumers'
dev/nix-shell 'just example-android test'
dev/nix-shell 'just android test-integration'
dev/nix-shell 'just example-android test-integration'
dev/nix-shell 'just example-android performance-check'
dev/nix-shell 'just example-android performance'
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
- B records narrow reaction controls at 200% text scale. F now records nine-screen action/scroll bounds at 320 dp/200%. The corrected
  Settings image has narrow reproof at F52e1. TalkBack remains pending.
- Test source and a green helper test do not prove the full V2–V8 scenario.
- Removing a dead app test requires its behavior and surviving callers to be
  checked. Keep `ExampleStorageTest`. Keep the SDK package, logging, lifecycle,
  attachment, and metadata tests.

## F native proof sources

`pr-f-native-restored-final-results/` records four tests, zero failures/errors/skips,
48.917 seconds of native case time. The matching recipe log is
`pr-f-native-restored-final.log` (1 minute 4 seconds). `pr-f-native-causal-matrix.json`
records all 14 exact assertions, methods and fresh XML paths. Each scope restores
production source and removes only its owned Android home, metadata database,
unsupported fixture and S3 relay. `pr-f-restored-cleanup.json` records the final
cleanup and lease release. No shared fault proxy was changed.

`pr-f-nine-controls-source-map.json` maps the nine group controls from F28819 to
the current D layout parent. `pr-f-nine-screen-restored/` retains the passing
action-bound images. My fields status is readable; the old Settings image still
contains the reset dialog's exit surface and is not a settled Settings proof.
The current test captures Settings before the modal, retains reset/Cancel actions
and adds explicit dialog-title absence. Final narrow scale reproof passes1/1,
zero failures/errors/skips at F52e1 (37.003 seconds native;51 seconds Gradle).
`pr-f-native-visual-final-results/` records the result;
`pr-f-nine-screen-final/` contains the actual nine final images. Root/F viewed
Settings without the modal. `pr-f-visual-final-cleanup.json` records exact source
and owned cleanup/lease release. No Group3 or unchanged full-suite repeat ran.

## Task 10 relocation checkpoint — 2026-10-09

The intermediate base is PR F commit `453fb17a7991ced63344714c6e44d60938462425`.
The relocation is on branch `codex/move-example-android`. This is not the final
F source freeze. Final history-page integration and the new performance cache
control remain with their upstream owners.

At checkpoint `aa509bf85f`, all 137 host/shared source files from this base had identical bytes at their
new paths. The package remains `org.xmtp.android.example`. The host is in
`apps/example-android/app`; the shared module is in `apps/example-android/shared`.
Runtime storage paths are unchanged. The app Gradle root maps the local SDK
module through an explicit composite build. SDK version pins, wrapper files,
and the dependency verification inventory have one source in `sdks/android`.

| Check | Intermediate result |
| --- | --- |
| Strict Off Debug/Release, shared targets, and app test APK | Pass |
| Strict configured Firebase build graph with build-only configuration | Pass; no Firebase delivery proof |
| SDK assembly after removal of app-only root plugins | Pass |
| Existing CI selector tests | 20 pass |
| New app/SDK/shared/mixed/docs selection and command-path tests | 8 pass |
| Full configuration checks | Pass with local repository tool dependencies |
| SDK package-script tests | Pass |
| Tiny NDK probes for four ABIs | Pass; these are not SDK JNI packages |
| Markdown and spec checks | Pass; spec check has 34 existing warnings |

The nine routing/path controls removed the app exclusions, dropped app
compatibility for SDK inputs, dropped the mixed SDK platform route, made app
docs unknown, disabled unknown-path fallback, disabled the draft mask, accepted
a skipped performance job, removed the app from the aggregate gate, or restored
the old screenshot path. Each control failed an intended assertion. Each
restored test passed. Local logs are kept in `.cache/relocation`.

Removing the local SDK composite mapping failed the actual app check recipe at
dependency resolution for `org.xmtp:android`. Restoring the mapping passed the
strict Off app check.

Earlier proof on remote F `a229aee004` includes 50 host tests per variant,
SDK public consumer checks, SDK/app lint, seven metadata fixture tests, and
eight performance-tool tests. These checks must be repeated after the final
source rebase. The old performance cache control is retained only as historical
source until the final new-cache control and path map arrive.

Pending: final F freeze and source map, final rebase, new-cache control,
independent review, moved native setup/launch/fixture proof, and fixed Linux
performance proof. The relocation PR is not open at this checkpoint.

The intermediate source review found three wiring defects: Cargo selected the
new non-Rust app directory, the SDK package validator still selected old APK
paths, and the active final command list used removed recipes. The fixes exclude
the app from Cargo, keep SDK AAR checks in the SDK root, check both moved APKs
in the app recipe, and update the command list. The command-path test exercises
both package inventories. Restoring the old SDK APK path or dropping app APK
inspection fails its intended assertion; restoring each fix passes.

The inherited host test expected restore in the same process after sign-out.
The exact fixture from E `fdc331bda8fdd8efd50d8333da11a79dc88d0e4b` now models a
new process with a new SessionFence. Only that test file changed. Production
session source still matches the intermediate F base. The full host rerun and
real four-ABI package build remain in progress at this checkpoint.
