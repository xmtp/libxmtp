# Messenger verification ledger

The historical app plan uses P1–P35 and V1–V9.
The redesign plan uses P1–P29. These are plan identifiers, not SDK
spec identifiers. Existing SDK tests remain SDK proofs. App proofs must call
the production app boundary. A scripted reducer result cannot prove a native
transfer, metadata commit, or persisted SDK state.

## Handle selection, 2026-10-13

Choose your handle (onboarding) and Change handle (Settings) follow the
approved handle designs. `HandleControllerTest` drives every editor state
through the production `HandleController` with a fake account: empty, invalid
grammar, too short, checking, a stale answer after an edit, a free bare name,
suggestions and Show more, a taken coded handle, no free handle, taken during a
claim with fresh suggestions, the first one selected, and no automatic claim,
not allowed, too soon, the claim limit, a failed check, a claim with no answer,
a switch back that fails the current rules, confirmation, Skip for now, and an
account that already has a handle. Each of the 22 tests failed under its own
mutation of the controller or the input check, and passed again after the
source was restored.

Review round 1 added three controller tests and one more page of Show more:
typing the current handle or another handle of the inbox, which the server never
lists as free; a too-soon answer when this device's last claim is older than
7 days; and an `InvalidArgument` from the SDK during a check. `HandleStepTest`
drives `HandleStep`, which keeps Choose your handle to one run for each account.
Each new assertion failed under its own mutation, and the 23 earlier mutations
still fail their tests.

The PD15 cleanup removed the editor's extra reads and their tests: the network
read and load error when the editor opens, the read after a claim with no
answer, the read after a too-soon or limit answer, and the ownership check of
earlier suggestions after a taken answer. The editor reads the handles this
device knows once. A taken answer gets a new lookup that never offers the taken
handle. A late answer applies only while its job is active, so an edit, open,
or close drops it. `HandleControllerTest` now has 31 tests. The new or changed
tests, a failed own read, a claim and a change with no answer, taken during a
claim, and an answer after close, each failed under their own mutation and
passed after the source was restored.

Review round 2 added three controller tests. A claim answer after close, before
the next open, changes nothing and does not leave the editor. A taken coded
suggestion is not offered again when the name lookup still lists it. The title
stays Change handle while the own handles load. Each failed under its own
mutation: no cancel in close, no taken-handle filter on the name lookup, and no
loading clause in the title.

`HandleStepInstrumentedTest` runs the production `AppGraph` and `RouteOwner`
against the worktree backend. A new account opens Choose your handle, and Back
there returns false. A restart before Skip for now opens the step again, not
the saved Settings route. After Skip for now, a restart opens Settings. It
passed on an owned ARM64 API 34 emulator with no proxies. It failed under each
of three mutations: Skip for now saves no flag, a saved route replaces the
pending step, and Back on the step opens Chats.

`ScreenScaleInstrumentedTest` now opens both handle screens, so the all-screens
assertion covers eleven screens. The exporter allows their two screenshot
names. Not run: the rest of app instrumentation, the scaled screen pass, and a
manual walk-through on an emulator.

## Messenger redesign, 2026-10-10

This section owns the architecture and design proof for the two redesign PRs.
Older sections below are historical evidence. Their source heads and passing
results do not prove this redesign.

### Architecture contracts

- AppGraph owns the process session, required message projection, typed feature
  controllers, route state, and Android effect requests. MessengerViewModel binds
  SavedStateHandle and screen state. Shell replacement creates no new default reader.
- AppSession receives explicit projection and lifecycle collaborators. Required
  message handling is awaited before reader advance. SessionObservers surround
  the real delegates; they cannot replace required processing.
- Connect, conversations, chat, group, settings, and attachment controllers own
  their state and commands. Shared Compose state contains no generated SDK object.
  Query adapters call the local composite SDK. Preferences hold small app records,
  not a second message store.
- Routes bind the profile and session generation. Delayed restore, old query
  completion, and old saved-state writes cannot replace a newer user destination.
  Completing Create opens Chat without retaining the completed Create screen.
- Send reserves the current draft before asynchronous work. One submission has
  one accepted native ID. Accepted-reference failure keeps the draft; publication
  failure exposes a fixed error and retries that ID. A newer draft edit survives
  acceptance. Reply restoration can read its native parent outside the loaded page.
- AttachmentController owns transfers and retained request identities. Activity
  hosts launch Android effects. Picker and Save results retain their session and
  destination identity through recreation. Open/Save use captured SDK-verified
  bytes. Session cleanup and final grant revocation remain required.
- Timestamp ties, opaque raw continuation, bounded history, accepted IDs, privacy
  revisions, notification admission, fixture ownership, and actual Release/JNI
  loading keep separate proof. No public SDK contract or package ID changes.

### Test reduction

Counts are declared methods. They include support tests and the manual benchmark.
They do not count assertion statements or parameter rows. Routine Android counts
include the optional configured-Firebase method; a skip must remain visible.

| Layer | Baseline | PR1 architecture | PR2 design |
| --- | ---: | ---: | ---: |
| Host JVM | 83 | 77 | 77 |
| Debug host | 1 | 1 | 1 |
| Release host | 1 | 1 | 1 |
| Routine installed Android | 138 | 137 | 132 |
| Manual installed benchmark | 1 | 1 | 1 |
| Release installed Android | 2 | 2 | 2 |
| Fixture process methods | 32 | 32 | 32 |
| Performance support methods | 22 | 22 | 22 |
| **App tree total** | **280** | **273** | **268** |

The six duplicate host methods and one Activity smoke method were removed in
PR1. The retained native proofs exercise the same contract through real SDK state.

| Removed method | Retained named proof |
| --- | --- |
| `MarkdownSelectionTest.markdownParentKeepsTextWithoutAFallback` | `MessengerReviewInstrumentedTest.nativeMarkdownIsVisibleAndKeepsItsReplyQuote` |
| `MarkdownSelectionTest.markdownUsesTheCurrentCodecTypeInPublishedAndUnreadSelections` | The same native Markdown proof checks history, unread selection, preview, and quote. |
| `PendingMessagePagesTest.emptyRawPagesKeepContinuationAtTheFourReadLimit` | `MessengerFeedbackInstrumentedTest.emptyNativePendingProjectionKeepsRawContinuationAfterFourReads` |
| `SDKHistoryPageTest.emptyConvertedPrefixUsesRawContinuationAndFourReadBudget` | `MessengerRaceInstrumentedTest.sdkRawContinuationReachesReadableRowsAfterAnEmptyPrefix` |
| `SDKHistoryPageTest.anchoredQueriesUseSdkNeighborsWithoutLookingUpTheDeletedAnchor` | `MessengerRaceInstrumentedTest.deletedNativeAnchorUsesItsRetainedTupleAndTheNextNewerRow` |
| `DraftDiscardStateTest.aCompletedDiscardRemainsUntilAllSnapshotReadersFinish` | `AttachmentRecoveryRaceInstrumentedTest.completedDiscardMarkersExpireAfterSharedSnapshotsFinish` |
| `MainActivityInstrumentedTest.messengerActivityLaunchesWithTheProcessSession` | `MessengerScreensInstrumentedTest.realSessionRendersSetupListMessagesAndSettings` also checks the installed package ID. |

PR2 carries the five installed journey consolidations in the next PR.

The nonterminal partial-page host case remains. Its terminal native counterpart
does not prove retained continuation. Timestamp-tie, cache-bound, accepted-commit,
reader replay, privacy, grant, release, and fixture tests remain independent.

### Architecture evidence

The full-suite baseline is `25e1ff5386a7d316000750567a1ab19d62085293`.
The complete Debug suite ran at `f8610a0987ed15fcb2c5c3d09e2c5bf432884ed5`.
The later rebase added only the API 23 CI memory change. A source comparison
confirmed no app difference. Release and host tests ran after that rebase.
The local run archive is `xmtp-android-redesign-20261010`.

Commit `ec52581121` then reused the route owner's conversation mapping and fixed
connect retry ownership. An older Connect or Retry completion cannot clear the
newer attempt's backend and credential. The existing
`MessengerRecoveryUxInstrumentedTest.newServerJoinClearsFailureAndUsesCurrentBackendAndCredential`
checks both overlapping operations after its actual native connection proof.
Both separate guard-removal controls failed at their named assertions. The
restored method passed with no skips. Evidence: `architecture-retry-restored/`,
`control-runs/architecture-connect-retry-owner/`, and
`control-runs/architecture-retry-retry-owner/`. Format and Android lint passed in
`format-architecture-retry-4.log` and `lint-architecture-retry.log`.

| Proof | Final evidence |
| --- | --- |
| Strict Debug/Release/shared/test APK and JNI inspection | `dev/nix-shell 'just example-android check'`: PASS. Both APK JNI inspections passed. Log: `check-architecture-final-2.log`. |
| Source format and Android lint | `dev/nix-shell 'just example-android lint'`: PASS. Log: `lint-architecture-final-2.log`. |
| Host Debug/Release | `dev/nix-shell 'just example-android test :example:testReleaseUnitTest'`: 156 executions passed, no skips. XML: `architecture-host-final-second/`. |
| Complete routine Android suite | `dev/nix-shell 'just example-android test-integration'`: 136 passed, one configured-Firebase skip, no failures. XML and 25 screenshots: `architecture-full-final/`. Owned emulator, fixture, and response gate were removed. |
| Actual Release instrumentation | `dev/nix-shell 'just example-android test-release-integration'`: two passed, no skips. This installed the signed Release build with `DEBUG=false`. XML: `architecture-release-final/`. |
| Configuration and fixture checks | `dev/nix-shell 'just lint-config'`: PASS. Log: `lint-config-architecture-final-2.log`. |
| Material source controls | All 20 controls below failed at their intended assertions. Each source file was restored and its hash checked. The complete restored Debug suite passed. XML and hashes: `control-runs/`; index: `architecture-controls-summary.json`. |

Seventeen controls ran at `7cef1b3bd0`. The labeled reader and route controls,
and the saved-server probe control, ran at `f8610a0987`. Original generic timeout
attempts for the reader and route controls receive no credit.

| Weaker behavior | Observed failure |
| --- | --- |
| Allow a second submission of one draft | Two native rows instead of one. |
| Omit the reply-parent lookup outside the loaded page | Missing native reply parent ID. |
| Detach required reader handling | Original native ID did not replay while unregister was blocked. |
| Retain chat A while loading chat B | Loading B did not hide A. |
| Admit a delayed send to the old chat | An extra native row appeared in A. |
| Clear the composer before accepted-reference persistence | The editable accepted body disappeared. |
| Hide publication failure | The publication error did not appear. |
| Create a new export on repeated Open | The export URI changed. |
| Drop Select ownership during recreation | The retained Select request was missing. |
| Admit a stale attachment capability result | Old support entered the replacement session. |
| Treat metadata edits as pending membership changes | An eligible push was rejected. |
| Omit the member-add privacy guard | A push was accepted during the pending member change. |
| Recover a send while its original submission is active | Publication recovery lost its expected error state. |
| Drop navigation while session binding is pending | The current chat did not open. |
| Tie Create completion to the old destination | The completed lookup kept its busy state. |
| Tie Settings completion to the old destination | The completed command kept its busy state. |
| Keep the marker after a rejected leave | The leave marker remained. |
| Keep the marker after a rejected promotion | The promotion marker remained. |
| Read native group state twice per row | Fifty groups caused 100 state reads instead of 50. |
| Omit the saved-server startup probe | The saved server did not receive its credential capability. |

Required handling uses the real AppGraph. Isolated session protocol tests use
one explicit test-only collaborator fixture. These controls prove their stated
contracts. They do not replace the complete installed suite.

#### Recovery regression proof

Commit `63a11d29b4` fixes three recovery paths. A missing request clears its
confirmation and shows an unavailable message. Retry reopens the current chat
when no matching native conversation is bound. Initial scroll restoration waits
for the saved native row, including when the saved anchor arrives after a
loading placeholder. Later viewport updates do not repeat the initial scroll.

The existing installed journeys cover each path:

| Journey | Source control and observed failure |
| --- | --- |
| `MessengerReviewInstrumentedTest.lateBlockedConsentCannotReplaceNewChatSettings` | `architecture-missing-request`: restoring the silent missing-request return keeps the Block confirmation set. |
| `MessengerRaceInstrumentedTest.suspendedOldOpenCannotReplaceNewChatOrSettings` | `architecture-unbound-chat-retry`: removing route reopening prevents the real Retry button from binding the native chat. |
| `MessengerRaceInstrumentedTest.anchoredRefreshPreservesEveryLoadedNativeMessage` | `arch-saved-anchor-delayed-history`: removing the restoration gate and row-availability retry changes the first UI viewport's saved native message ID. |

All three journeys pass after source restoration: three tests, no failures,
errors, or skips in `architecture-review-restored-final/`. That archive records
the source diff and file hashes. Each control records its exact source, test,
mutation, failure, and restored hash under `control-runs/`. The scroll journey
also checks the saved pixel offset and persisted cursor after the exact viewport
action completes. No test method was added.

Strict builds and both APK JNI checks passed in `check-architecture-review-2.log`.
The final test source compiled in the installed run. Format and Android lint
passed in `format-architecture-review-6.log` and
`lint-architecture-review-final.log`.

### Design baseline evidence

The restored full Debug suite uses source
`61401309df3d7e69dba1a6a08ad3a9586c779273` with an empty source diff.
Its archived files match that recorded baseline source. Other rows retain their named
tested source scope.

| Proof | Recorded result |
| --- | --- |
| Complete baseline Debug suite | 132 cases: 131 passed, one configured-Firebase skip, zero failures or errors. XML and 27 exported PNGs: `ui-full-restored-final/`. Source/XML checks and PNG hashes: `ui-restored-full-verified.json`. |
| Baseline host Debug and Release | `dev/nix-shell 'just example-android test :example:testReleaseUnitTest --rerun-tasks'`: 156 fresh executions passed, zero failures, errors, or skips at `61401309df`. All 266 archived source hashes match. XML: `ui-host-current-final/`. |
| Baseline actual Release instrumentation | Two tests passed with no skips at `61401309df`, with an empty source diff. All 266 archived source hashes match. XML: `ui-release-current-final/`. |
| Baseline format and Android lint | PASS at `61401309df`. Log: `lint-ui-current-final.log`. Release/host/lint evidence checks: `ui-final-gates-verified.json`. |
| Actual large-text scale journey | `ScreenScaleInstrumentedTest.allScreensRemainReachableAtDoubleTextScale` passed in `ui-focused-sixth/`. The combined route failed its separate screen method. This is one passing named case, not a passing combined run. |
| Main screen journey | `MessengerScreensInstrumentedTest.realSessionRendersSetupListMessagesAndSettings` passed at clean `41c370843c671a64b21581b7849473cf2797de2a` in `ui-screens-eighth/`. |
| Native picker render and retention | The same main screen method passed with the picker fix in `ui-native-picker-square-positive/`. It detected 24 drawn emoji cells and retained the native grid after a state update with unchanged columns. Fourteen screenshots include the four rendered variants. |
| UI source controls | All 18 controls failed at their intended assertions on the recorded pre-restack source. Target names, XML, patches, source/test hashes, and restoration were checked. Frozen index: `ui-controls-ee7491-summary.json`. This is control proof, not a complete restored suite. |

The initial picker render check failed at `native picker draws visible emoji
cells` in `ui-native-picker-positive/`. The same test SHA-256
`994168b7571d57effb60d4b2027b6162fa91f987e42f3f202fc1dcfdf41c848d`
passed after the host stopped setting an unchanged column count and stopped
forcing five rows. The passing picker source SHA-256 is
`9c59c5dc04c889856b3a388f4418366cc6aa9422eb58896002c4d0852bbf3a7d`.
This positive run used `41c370843c` plus the recorded picker diff before rebase.
The root agent inspected the full grid and all four variant images from
`ui-screens-eighth/`. Source hashes and diffs retain the exact run scope.

The complete run at `718f268bb7` had one Retry failure in
`MessengerRaceInstrumentedTest.suspendedOldOpenCannotReplaceNewChatOrSettings`.
The general failure banner now dispatches ShellAction.Refresh so Retry can
reopen an unbound native chat. The same method passes in the complete restored
run at `61401309df`. That run has no other failure or error.

The root agent inspected the current inline reactions, reply composer, native
emoji grid, dark Settings, RTL at 200% text, landscape with IME, and wide Chat
PNGs. The expected controls and content render.

### Request consent proof

Unknown-consent requests require Accept before composer editing, sending,
and outbound message or reaction actions. Copy, reactor lists, and Close remain
available. Reactor detail Add and Remove require accepted consent.
ChatController rejects retained outbound callbacks while consent is Unknown.
The native emoji callback checks current consent before dispatch.

The existing main screen journey checks disabled controls by retained draft
text, rejected retained callbacks through native consent and reactions, and
successful actions after Accept. No test method is added. Declared methods
remain 268. The earlier focused positive run retains its seven-file scope.

All three source controls failed once at their intended assertions, with zero
errors or skips. Production sources were restored to their recorded hashes.

| Control | Source | Intended first failure |
| --- | --- | --- |
| `ui-request-composer-consent` | `apps/example-android/shared/src/commonMain/kotlin/org/xmtp/android/example/shared/feature/chat/MessageComposer.kt` | Draft editor is enabled at line 672. |
| `ui-request-reaction-controls` | `apps/example-android/shared/src/commonMain/kotlin/org/xmtp/android/example/shared/feature/chat/ChatScreen.kt` | Quick reaction is enabled at line 687. |
| `ui-request-retained-reaction-consent` | `apps/example-android/app/src/main/java/org/xmtp/android/example/messenger/feature/chat/ChatController.kt` | Native consent is ALLOWED instead of UNKNOWN after joined callbacks at line 705. |

The unchanged test SHA-256 is `621be6c13fd9d3f15fbb2fb79ac44a26e553319c0777616ed46969d88b6891d6`.

| Control | Restored source SHA-256 | Mutant SHA-256 |
| --- | --- | --- |
| `ui-request-composer-consent` | `a8d70f929949c74dcee4bde9d88e5e32816f179fe25c79c4b84a47152622f5da` | `4555831cc6a8e77b8752e3e80fb702a76016a7dab87440955ace031b720f8a93` |
| `ui-request-reaction-controls` | `2c4ceb73e94c7ed2718306f9a123039d31d66ecbe14f6c0bb56f9022b9711939` | `2d86675481ef04126e80058aaf1a00f08c23d70f34380f8fd2521344370a56a0` |
| `ui-request-retained-reaction-consent` | `23277943a4d9b8dcba5a52d62addc13abd0ba0b0a29a3d13df5134af4524198d` | `642f8f0f83a5dbe411d6d5b26b319ebc2fe1d7f8e1b2c8cf5a92c5f092ee284d` |

The recorded full Debug suite uses clean source
`706e88b9e5ba65185bceec9831c425bb45ef2fca`: 132 cases, 131 passed, one expected
configured-Firebase skip, zero failures or errors. XML and 27 exported PNGs are
in `ui-request-consent-full-restored/`. The consent verifier passed all three
controls and the complete source/XML checks. The gate archive records identical
before/after head and all 266 source hashes, with no production changes during
the full run, evidence check, strict build, or lint.

Strict Debug, Release, shared UI, and test APK builds passed. Both APK JNI
inspections passed. Spotless and Android lint passed. Evidence is in
`consent-final-gates.json`, `check-ui-consent-final.log`, and
`lint-ui-consent-final.log`. The newly exported images are not claimed as
visually inspected. The earlier inspected images retain their baseline scope.

The 156 host and two actual Release passes at `61401309df` remain baseline
proof. They were not repeated for the UI consent repair at `706e88b9e5`.

### Live consent admission

Outbound queue, publication retry, and deletion read current native consent
before their native write. ALLOWED is required. Session and destination checks
surround that read. Queue checks after preparation, retry after the publication
wait, and deletion after its message read. Attachment preparation runs before
the shared queue check, so its last mutex wait cannot bypass live admission.

A rejected queue or preparation/consent-read error rolls back its QUEUEING
record under NonCancellable and keeps the draft. Rejected retry keeps its
accepted publication recovery record. Dead attachment command paths and their
unused projection work are removed.

The existing main screen journey uses two helpers to hold outbound work and
change real native consent. Its native assertions cover queued text/reaction,
retry, and deletion rejection before the rendered Accept flow. No test method
is added. The focused main screen and picker journeys retain their separate
source/run scope. Earlier full Debug and host results retain their stated
source scope.

Admission and the native write are separate operations. An independent consent
change after admission is outside this repair. No atomic guarantee is claimed.

The focused main screen journey passed one test with no failures, errors,
or skips in `ui-live-consent-positive-third/`. Its recorded scope is c058 plus
the live-consent repair. The native picker focused journey passed separately
with unchanged production. Neither focused result is a full-suite result.

The `ui-request-live-native-consent` control failed once at
`queuedConsentAdmission`, line 136: native consent became ALLOWED instead of
remaining UNKNOWN. There were zero errors or skips. The restored source and
archived test hashes retain their control scope. The production source is unchanged.

Restored SendCoordinator SHA-256: `5f1f7136b132c7eb9670e7b2a11377989f325f4056b52fc1c2e65055a0117928`.

Mutant SHA-256: `f9c149ee785241579f79d3987de0e174b55c332f305c7fda1135be82154552ae`.

Archived control test SHA-256: `fa6fdaea158a8e352e25e72eb0fde5fb741f16f0093cd8fe7408a0e5e28e0444`.

Final source is `ebc5756f08d2e89388511d678f3d9a563937c9d2`.
The complete installed Debug suite has 132 cases: 131 passed, one unchanged
configured-Firebase skip, zero failures or errors. Fresh host Debug and Release
tests pass all 156 executions with no skips. Strict Debug/Release/shared/test
APK builds, both APK JNI checks, Spotless, and Android lint pass. All 266 source
hashes and the clean head are unchanged across these gates.

The read-only live-consent verifier passes the source control, complete XML,
and diagnostic equivalence checks. The archived native oracle is line 136 with
test SHA-256 `fa6fdaea158a8e352e25e72eb0fde5fb741f16f0093cd8fe7408a0e5e28e0444`.
The final diagnostic test is SHA-256
`b7932a0f070da3775a73d9d6167bdb5539410ecd37b2d262cdf671a11ae44d39`, with that
same native oracle at line 162. Removing only the diagnostic awaitBarrier helper
and restoring the two 30,000 ms waits reproduces the archived test bytes.

Final archives are `ui-live-consent-full-restored-third/`,
`ui-live-consent-host-final-third/`, and `live-consent-final-gates-third.json`.
The earlier full runs contain one fixture precondition failure and one queue
wait timeout; neither receives passing credit. The fixture sets ALLOWED after
the UNKNOWN lagged-event assertions before its separate early-send phase.
Actual Release instrumentation remains scoped to the two passes at `61401309df`.

### Local start command

`dev/nix-shell 'just example-android start'` was checked with the existing local
backend and object store. The command cold-booted a visible API 34 emulator,
built Debug with strict dependency verification, installed only on its selected
device, forwarded both service ports, and opened MainActivity successfully.
Android reported 1080 × 2400 pixels and an effective density of 420 dpi. The
captured PNG has the same dimensions and the rendered Start screen reported
Server ready. Pressing Enter stopped the owned emulator and removed its temporary
Android home. Configuration, Markdown, shell syntax, and ShellCheck checks passed.

This proof covers the launcher. It does not add app feature test coverage or
replace the source scopes of the app suites recorded above.

### Stable manual gaps

- Real FCM receipt, background delivery, successful OS posting, and tap need a
  configured developer Firebase project and a backend channel. Off behavior,
  compilation, and a captured publisher do not prove delivery.
- TalkBack service traversal, focus announcements, and switch/keyboard operation
  need manual device proof. Fixed Compose previews do not provide it.
- The fixed Linux API 34 x86_64 performance workload, its cache failure control,
  and restored same-dataset result need their explicit manual route. Functional
  ARM64 tests and host validators do not prove its timing or heap limits.

- Physical predictive Back from both edges and complete IME/system-bar behavior
  across OS versions need device proof beyond dispatcher-driven integration.
- All-screen dark, RTL, landscape, wide-window, long-name, and reduced-motion
  interaction needs further manual proof. The selected rendered journey above
  has a smaller scope.
- Edit-to-preview and test speed have no measured comparison on one machine and
  cache state. Package separation and fewer methods do not establish a speed gain.
- SDK receipt semantics, typing, presence, calls, invitations, mixed pending
  timestamp policy, and inbound private-download policy are not added by this
  redesign. No missing server capability is represented by invented UI state.

## Historical evidence before the redesign

The historical parent is `ada6699f3e9f32bdfc50464a0ed15f73a9e20d57` on E
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
dev/nix-shell 'python3.11 -B -m unittest discover -s apps/example-android/performance -p "test_*.py"'
dev/nix-shell 'just example-android performance'
dev/nix-shell 'just lint-config'
dev/nix-shell 'just test dev'
dev/nix-shell 'just spec-check'
```

Keep the selected API 23 and all-ABI package jobs. The app instrumentation
recipe must own its emulator, fixture process, forwarding, and teardown.
Benchmarks are explicit-only. Keep the fixed job in the workflow_dispatch-only
`benchmark-example-android.yml`; normal Android CI has no benchmark dependency.
Normal app tests exclude the workload unless `messengerPerformance=true` is set.
Host runner validators remain automatic and run no workload.

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

The fresh-process fixture rerun passes 52 tests in each of Debug and Release,
with zero failures, errors, or skips. Full configuration and Markdown checks
pass again. The real APK check found that the new app root did not apply the
existing Nix NDK path/revision configuration. The old build produced `.symtab`
in the APK's SDK JNI library and failed inspection. SDK and app roots now apply
one shared `gradle/android-ndk.gradle` script. Both actual app APKs pass after
restoration. SDK assembly builds both AAR variants; both pass JNI inspection.
All four Android native targets built successfully. Matched Kotlin generation
and final four-ABI package staging remain pending.

## Final F262 moved-source and runtime checkpoint — 2026-10-09

G `08bafbf7a5b1dd9fe9ecbabe946609ae091f1a8a` is based on F
`262f9366fd81c144fe7a668d5187fb5a27a1fce8`. All 140 moved host/shared source
files match F bytes. This includes the SDK history adapter/cache, deletion
predicate, fresh-process fixture, and PerformanceSeedCheckpoint helper.
The runner keeps SDKTranscriptCache as its active eviction control. The old
TimestampBuckets source is only the explicit rejected-target fixture. Moved
paths retain APK installation, seed checkpoint JSONL, same-dataset controls,
500 rows, three caches, four reads, and all stated performance budgets.

Strict Debug/Release/shared/test-APK build and actual APK JNI inspection pass.
Host suites pass 56 tests per variant with no failures, errors, or skips.
Configuration, Markdown, and all ten performance-tool tests pass. Kotlin
generation and matched four-ABI Android staging pass. Both SDK AAR variants
pass JNI inspection. Current proof logs are in `.cache/relocation`.

The moved native smoke passes four cases with zero failures, errors, or skips:
MainActivity launch, both SessionLifecycle profile/reset cases, and the real
operator-catalogue roundtrip through two SDK clients. The first attempt failed
one fixture assertion because the test environment set both profile URLs to
the same loopback value. The corrected environment uses the expected emulator
URL for BuildConfig and loopback as the fixture's second URL. The app source
is unchanged. The restored four-case suite passes.

XML is retained at `.cache/relocation/native-restored-results`; the exact source,
head, and cleanup record is `.cache/relocation/native-restored-proof.json`.
Both owned Android homes are removed. The catalogue config, backend process,
owned unsupported fixture, and S3 gate are removed. PostgreSQL reports zero
remaining messenger_metadata fixture databases. The runtime lease is released;
no shared proxy mutation occurred.

Post-push parent repairs can advance F. Final parent sync and a fresh integration
review remain open before PR submission. Full Linux performance and real
Firebase delivery remain pending; this checkpoint claims neither result.

## Relocation submission proof, 2026-10-10

The move consumes pushed F `27ca931436cd1aa68134986c67184791397399ac`.
The tested G source is `f06e99a220d5779331a35cdbdb96655934fc80d1`.
All 158 Android host and shared source files match F byte for byte, with no
missing or extra source. Fixture and performance scripts retain the parent
behavior after their root, recipe, and output paths change.

| Check | Result and scope |
| --- | --- |
| Strict app builds | Debug, release, shared UI, test APK, and build-only Firebase graph pass. |
| Host tests | 70 tests pass in each app variant; zero failures, errors, or skips. |
| Native packages | Both SDK AARs and both app APKs pass JNI stripping for all four ABIs with matched S source `11c39dc2` and release contract `e6e65a3d`. |
| Configuration | 133 actual host cases pass, including eight moved selection/path cases and seven private-export fixtures. |
| Failure controls | Missing APK retention, an old export path, and a column listing each fail at an intended assertion with zero test errors; restored selection/path cases pass. Earlier routing, package, and XML controls remain retained. |
| Moved native boundary | 16 cases pass on the owned ARM64 API 34 emulator. They cover launch, sessions, metadata, recovery, rollback, accepted-ID commit, password input, state reads, scale, and the attachment card. |
| Private export | Ten actual PNG files export and decode at 320×640. Nine are scale screens. The owned private proof directory is removed. No public image is added. |
| Actual CREATE interruption | SIGINT, SIGTERM, and uncertain command result each remove their exact committed fixture database. |
| Cleanup | The owned emulator home, fixture containers/network, and exact catalogue/CREATE databases are absent. The S3 gate is removed. The full proxy JSON is unchanged, with zero shared writes. |

Native XML, PNG hashes, CREATE results, and cleanup proof are retained in
`.cache/relocation/27ca-native-results`. Live process IDs were not captured;
the post-cleanup process scan found no owned wrapper, emulator, or metadata
backend. Source maps, host XML, and control logs remain under
`.cache/relocation/`.

Fresh independent review is required before first submission. Current-head
Linux performance and CI remain separate pending gates. The earlier F635
measurement pass is not a result for this source. Real Firebase delivery and
manual TalkBack remain unverified.

The two product decisions about mixed pending display and a proposed SDK
attachment download policy remain open. They do not change this relocation's
published interfaces or establish that the full stack is ready.

## Parent and routing update, 2026-10-10

G now consumes pushed F `2d6d16c0cf5155e506d6855e385613f92199facf`.
All 158 moved source files match this parent. Its four-file delta carries the
accepted-ID recovery fix, its native test, the mapped caller, and the scale
setup flag fix. SDK source `11c39dc2` and release contract `e6e65a3d` are unchanged.
The strict moved build and both 70-case host variants pass on this source.
The parent has native two-case failure/restoration proof for the accepted-ID
fix and causal resource proof for the scale flag fix. The earlier moved
16-case runtime and fresh relocation review remain scoped to F27ca.

Round41 finding `PRRT_kwDOI5fJCc6rCjoE` is confirmed at G042556 and on the
rebased source. SDK Gradle properties and wrapper files were unknown inputs,
so the selector enabled every gate. The shared Android input filter now
includes the SDK Gradle tree, root properties, and wrapper scripts. Nine
selector/path tests verify Android and app compatibility selection without
unrelated platform jobs. The old filter causes seven intended assertion
failures with zero test errors; the restored filter passes all nine cases.

## Final parent and exporter update, 2026-10-10

G consumes F `ce906d323d350c6b1156578df00dd155448c6b2c`. All 158 moved app and
shared source files match this parent byte for byte. The exporter matches F
byte for byte. The performance runner differs only in four relocation paths.
The parent repair preserves remote command errors and records seed evidence
with its workload identity. It does not change native code, timers, or budgets.

The earlier local F707 checks use SDK source `13513024` and release contract
`6f754f78`: strict app builds, all four ABI package checks, and 72 host tests in
each app variant pass. Three actual CREATE cleanup cases, disposable target
admission and health, and one native metadata roundtrip also pass. The metadata
route then failed in the old exporter. That failed route remains recorded.

On rebased G `1b987ff11b1372330a3b0930f16f774bfeb8f620`, the actual moved
exporter returns zero for an installed app with no private proof directory.
It removes stale owned host PNGs and keeps an unrelated file. An inaccessible
app still raises the original remote error. The owned app, output directory,
emulator home, and emulator process are removed. This small check does not
repeat the metadata test, backend workload, or screenshot suite.

The first rebased configuration check failed because G's recipe test used an
old fake-device listing command. The fake now requires `shell -T` for listing
and cleanup, and `exec-out` for binary PNG reads. The old production exporter
fails this actual recipe test; the restored exporter passes all nine path and
selection cases. The final configuration run includes the parent exporter and
performance host cases. Proof files remain in `.cache/relocation/ce906-*` and
`.cache/relocation/final-export-*`.

CI artifact `11665637758` belongs to remote G `39d7315`, before this parent
update. Its normal performance run passes the fixed 1000-group and
100000-message workload. The cache failure-control run hits the older-transcript
timing limit before the required cache assertion. The runner rejects that
result and no restored pass runs. A separate kache stop timeout is recorded.
Final CI and full Linux performance on the new parent remain required.
