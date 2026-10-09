# Messenger verification ledger

The approved app plan uses P1–P35 and V1–V9. These are plan identifiers, not SDK
spec identifiers. Existing SDK tests remain SDK proofs. App proofs must call
the production app boundary. A scripted reducer result cannot prove a native
transfer, metadata commit, or persisted SDK state.

The combined source check starts at `1e2a9aee1c8084d88fd65d62a5456904057d7506`.
Its exact base is E `7b25ca53261da655a014f1ee29a245c2ec288540`, D
`249fbad6ae4abee9d02a71ab1d0f2f64fa7fd593`, C
`05164d23a51ee7575967d1f965669cac9f2fae31`, and B
`8c4929abe3ecf363b4c46f19a198c2b324480215`.
This source includes B's Markdown, recovery-page and owner-projection repairs
and E's fresh-session native fixtures. E's repaired native run and controls
remain pending. F's four new device cases and V9 remain unrun.

This ledger separates current source checks from recorded branch runtime proof.
A listed test or compile is not a passing native result.
Fill each evidence cell with the source commit, command, result, and artifact
before the final app gate is ready. Label every manual result as manual.

| Requirement | App proof and required observation | Current evidence |
| --- | --- | --- |
| P1 Current exports | V1. Public `SDKClient` and records; no Rust export, schema, raw client, or app SQL addition | A package/build proof recorded; final source audit pending |
| P2 Shared UI | V1. Compile actual commonMain Compose screens and Android host under strict locks | Current combined strict app/shared/test APK compilation recorded; native and final CI gates remain pending |
| P3 Navy design | V8. Screen actions, 48 dp controls, light tokens; manual design, text scale, TalkBack, keyboard, empty/error cases | B216304 records reaction controls at 200% on a narrow display; the new nine-screen 320 dp/200% test compiles but is unrun; TalkBack remains pending |
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
| P17 Stored retries | V4/V5. Accepted ID survives recovery; retry publishes that ID once; no typed resend | B/C native accepted-ID recovery pending |
| P18 Unknown send | V2/V5. Interrupted QUEUEING lacks ID; review/discard; no automatic queue; old screen queue completion fenced | B/C process and queue-outcome proof pending |
| P19 Removal/expiry | V4. Loaded content disappears or shows supported deleted placeholder after real delete and expiry | B device expiry and refresh proof pending |
| P20 Groups | V4. Create presets, name/description, members/roles, disappearing, failed intermediate preset, accurate PendingRemove | Three real shared-settings/ViewModel cases compile for edits, member/admin changes and PendingRemove; their native assertions and controls remain pending |
| P21 File staging | V5. Missing/lying provider length, 64 KiB chunks, effective ceiling, identical names, private source cleanup | Ca675 records provider/copy/ceiling native proof; Cbd26 retains the same app/test source on B80183; current F native replay remains pending |
| P22 Upload first | V5. Failed/retried upload, queue after Complete only, discard active upload, no stale-screen queue | Ca675 records real upload/retry/discard and Complete queue admission with matching controls; current F native replay remains pending |
| P23 Draft recovery | V5. Encrypted full descriptor; Complete reopen with pending(remote); short-age expiry; orphan; accepted ID takes precedence | Ca675 records SDK/client/coordinator reopen, pending expiry and Complete admission; literal OS process kill is not claimed; current F replay remains pending |
| P24 Verified files | V5/V6. Digest verification, corrupt/missing object errors, sampled local preview, external receiver only gets one file, revoke/reset denies read | Ca675 records actual verified card, external UID grants, revoke/reset and final one-filename image; current F native replay remains pending |
| P25 Field discovery | V7. Committed descriptors by componentId; changed label keeps identity; unknown type has no read/write; offered missing field explained | D1d5 records real controller/shared UI catalogue reads and unknown/type/policy host cases; current F native replay remains pending |
| P26 Group values | V7. String/Bytes/map/set commits through app; delta keeps unrelated entries; limits and denied policy enforced | D1d5 records actual peer readbacks and group mutations with matched native controls; current F native replay remains pending |
| P27 Own fields | V7. One changed-only updateUserData; own refs only; empty differs from Clear; group/DM values isolated | D1d5 records actual group/DM own and sibling values, dirty refs and draft preservation; current F replay remains pending |
| P28 Field refresh | V7. Events and failed/type-changed/policy-changed saves reread; actual error retained; stale completion rejected | D1d5 records actual error/refresh/admission/event/draft controls and restored native cases; current F replay remains pending |
| P29 Optional Firebase | V8. Unconfigured strict build, transport Off, no registration or permission request; configured build also compiles | E7b25 passes both strict builds and app lint in this checkpoint; repaired native fixtures and full controls remain pending |
| P30 Generic push | V8. Parse ULong, current known group/installation admission, mute/consent/token policy, dedupe, late A push after B sign-in, generic content | E records host policy/restore/transport controls; its first native run is incomplete; real FCM needs a developer project/channel/device |
| P31 App gates | V1/V9. Current SDK host/package/consumer/platform gates plus actual launch and app tests in scoped emulator; no release publication | Final recipes, CI, and combined gate runs pending |
| P32 Failure detection | V1–V9. Each new test has a plausible broken production run and restored pass at a recorded source commit | Host performance result gate has broken/restored records; final per-test ledger pending |
| P33 Performance | V9. Exact 1000/100000 workload, one 50000 transcript, five warmups, 30 measured runs, all query/heap/retention limits | Test APK compiled; eight host validator/control checks and the generated-message wrapper regression pass; real Linux fixed-device run pending |
| P34 Credential visibility | V2/V8. Current server auth configuration shows the credential field only for its URL; stale capability results cannot show or hide the current field | B216304 reads the actual server with authentication disabled; scripted required-authentication and stale-URL branches have a matched control; final integration remains pending |
| P35 Attachment availability | V5/V8. SDK and current server support determine attachments; Start has no attachment checkbox or network switch | B/C final UI restack and screenshot proof pending |

## Current combined source checks

The current combined source passed strict Off and synthetic configured app/shared/
test APK builds, 48 app host tests and actual app `:example:lintDebug` in each
variant, eight performance host checks, default format, SDK Android lint,
70 config checks and Markdown lint. The final artifacts use Off.
Synthetic Firebase resources were removed. These checks use the base named above.

E's separate configured/Off publishers resolve the prior app lint failure.
The configured publisher checks permission before `notify`; the Off publisher
does not post. Both actual app lint variants pass. No suppression or Off permission
declaration was added. This does not prove device permission or notification behavior.
No new native or backend result was produced by this combined source check.

## Recorded branch evidence

These results are from the separate branch reports. They do not prove the final
combined tree. The parent must verify the final restack and current source.

| Branch | Recorded checks | Remaining proof |
| --- | --- | --- |
| A shared build | Strict app/shared/SDK assembly, consumers and selected platform checks recorded in PR A | Combined final gates |
| B8c | Prior B1091 has 43 debug and two release native cases; B80183 has two harness cases; B8c has four restored repair cases, a final two-case refinement and host24 in each build | No fresh full 46-case suite; current F replay, Linux confirmation, TalkBack and manual checks for all screens |
| C05164, prior Cbd26 proof | Seventeen native cases at Ca675, with actual Complete queue admission, concurrent reply preservation and verified card/grants; all four app/test source trees match Cbd26; host23 and strict/lint gates pass | Current F native replay |
| D249fb, prior Dfea8 proof | Host35, including 12 metadata cases; two actual controller/UI native cases and eight matched controls at D1d5, with retained source hashes | Current F native replay |
| E7b25 | Current F records both strict variants and app lint with E's publisher repair; synthetic public resources prove compile only | E's first native run has seven passes and three fixture failures; fresh-session fixtures compile, but repaired runtime and native controls remain pending, as does real Firebase delivery |
| F combined source | Current strict Off/configured builds, host48 and app lint in each variant, performance host8, SDK lint and config70 pass on the named base | Four new native group/scale cases and controls; exact Linux V9/cache failure/restoration; observed seed time must set the timeout |

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
Current F source checks use `pr-f-676-*` logs.
`pr-f-676-source-evidence.json` records identical performance and prepared UI
proof files across this restack; `pr-f-676-vm-merge-diff.txt` records the VM delta.
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
and real SDK staged drafts. It uses physical swipes, checks scrolling progress,
48 dp action bounds and label clipping, and saves each screen. It restores the
previous display settings after Activity teardown. This source is compiled but
has no device result. It does not provide TalkBack service proof.

Both classes need native green, intended production-control failures and restored
passes on the final stack. The prepared controls omit real group writes and user
scrolling. Compiler errors and fixture timeouts do not count as failure proof.

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
