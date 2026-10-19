# React Native example

Run from the repository root. Recipes select the `react-native-local` Nix
shell, which uses the selected Xcode. The canonical `react-native` shell
requires `/Applications/Xcode_27.0.app`. On Linux the two are the same.

- `dev/nix-shell 'just example-react-native check'` typechecks, lints, checks formatting and runs the unit tests. It links the JS-only SDK when no SDK is linked. CI runs it on pull request pushes that change the app (not only Markdown) or carry the `react-native` label, drafts included, on full runs, and on every merge to main. Markdownlint checks the app's Markdown; Oxfmt skips it.
- `dev/nix-shell 'just example-react-native format'` formats the JS, TS and JSON sources.
- `dev/nix-shell 'just example-react-native sdk'` links and verifies the native SDK package. `sdk-js` links the JS-only product.
- `dev/nix-shell 'just example-react-native install'` installs JS dependencies from the lockfile and the pods.
- `dev/nix-shell 'just example-react-native metro'` starts Metro on `XMTP_RN_METRO_PORT` (default 8081).
- `dev/nix-shell 'just example-react-native build-ios'` builds the simulator app into `.build/DerivedData`. `CONFIGURATION=Release` embeds the Hermes bundle and needs no Metro.
- `dev/nix-shell 'just example-react-native start-ios'` builds, installs and launches on `XMTP_RN_SIMULATOR` (UDID or name; default the first iPhone). Append `--headless` to keep Simulator.app closed.
- `dev/nix-shell 'just example-react-native build-android'` builds the APK with the Nix Gradle.
- `dev/nix-shell 'just example-react-native start-android'` builds and runs on a scoped API 34 emulator from the `android` shell. Append `--headless`, or `--screenshot FILE` to save the first screen and stop.

## Parallel checkouts

A Debug build loads JS from the Metro port it was built with: `build-ios`
stamps `XMTP_RN_METRO_PORT` into Info.plist (`RCT_METRO_PORT`), and
`build-android` sets `reactNativeDevServerPort`. Give each checkout its own
port and simulator, and start its Metro before `start-ios`:

```bash
XMTP_RN_METRO_PORT=8092 dev/nix-shell 'just example-react-native metro'
XMTP_RN_SIMULATOR=<udid> XMTP_RN_METRO_PORT=8092 dev/nix-shell 'just example-react-native start-ios --headless'
```

`start-ios` refuses a Metro that serves another checkout's app.

## Launch environment

JS cannot read the process environment. `src/platform/messengerPlatform.ts`
reads `XMTP_BACKEND_URL`, `XMTP_ALLOW_PRIVATE_ATTACHMENTS` and
`XMTP_UI_TEST_ATTACHMENT` through the local module: on iOS from
`SIMCTL_CHILD_<NAME>` at `simctl launch`, on Android from `am start --es`
extras. Another app cannot set an iOS process environment, but it can start
the exported Android activity with any extras, so Android reads them only in
a debuggable (Debug) build. A Release APK ignores them: it opens Connect at
the default address and never allows private attachment hosts. The start
recipes pass this worktree's backend unless `XMTP_BACKEND_URL` is set.
Private attachment access needs exactly `1`.
Android reaches the host backend and object store with `adb reverse`; set
`XMTP_S3_PORT` when the backend belongs to another worktree.

## SDK

`package.json` depends on `file:./.build/sdk/package`, a gitignored link to a
Nix product. pnpm never refreshes a directory dependency, so `install`
removes `node_modules/@xmtp/react-native-sdk` first. The native package is an
x86_64-linux output that needs Xcode 26.3: a Mac only substitutes it from
Cachix. When this checkout's SDK identity is not cached (for example after a
root `Cargo.toml` edit, which the identity covers), `sdk` fails fast. Use the
package of a merged revision whose SDK content matches:
`XMTP_RN_SDK_REV=<rev> dev/nix-shell 'just example-react-native sdk'`.

## Code

- `app/` holds expo-router route files only. Each reads params and renders a feature screen. No platform suffixes in `app/`.
- `src/core/` is pure TypeScript with no React, React Native, Expo or SDK runtime imports (type-only SDK imports are fine). Its files import each other with explicit `.ts` extensions, and `node --test` runs `src/**/*.test.ts` directly.
- `src/platform/` wraps Expo modules and the local module `modules/messenger-platform` (Swift and Kotlin, autolinked from `./modules`).
- `src/ui/<Name>.ios.tsx` holds native iOS leaves; `src/ui/<Name>.tsx` is the plain React Native version for Android.
- Import app code through the `@/` alias (`src/`). Oxlint rejects relative parent imports in `src/`.
- Keep copy in each feature's `copy.ts` or `src/copy/`. Keep `testID` and `accessibilityLabel` values stable: UI checks select by them.
- Hermes has no `crypto.getRandomValues`, `TextDecoder` or `Buffer`. Pass native paths, never `file://` URIs, to the SDK. 64-bit SDK values are `bigint`; persist them as decimal strings.

## Design system

`src/ui` is the design system. Its metrics were measured on an iOS 27 iPhone
17 Pro.

- Colors come from `useColors()` or a palette name on `color` props; never
  hard-code hex values in screens. Text uses `Text` variants (SwiftUI text
  styles with Dynamic Type); icons use `Symbol` (SF Symbols, Material Symbols
  on Android).
- Buttons are `GlassButton` and `GlassIconButton` (native SwiftUI glass on
  iOS). Grouped screens use `FormScreen` + `FormSection` + the rows in
  `FormRows.tsx`; make `FormScreen` the route's first view so the large
  title collapses. A form with text fields passes `avoidsKeyboard`, not
  `automaticallyAdjustKeyboardInsets`: React Native clamps that offset
  without the bar's inset, so any keyboard change moves a resting form
  under the bar.
- `Stack.Toolbar.Button` icons go through `useToolbarIcon()`, because Android
  needs an image. A titled toolbar button (Next, Create) is
  `ToolbarTextButton`: Android draws no titled `Stack.Toolbar.Button`.
  Pass `confirmation` for a confirming action (semibold Save, Add). A
  toolbar that also holds icons uses `toolbarTextItem()`, because
  `Stack.Toolbar` reads only its direct `Stack.Toolbar.*` children on iOS.
  `NavigationSubtitle` sets the iOS navigation subtitle.
- List rows with a long-press menu use `ContextMenu` (a UIKit context menu
  in `modules/messenger-platform`; Android renders the children only).
  Confirmations that SwiftUI shows as popovers (`.confirmationDialog` on a
  list) use `PopoverConfirmation`. SwiftUI `.alert`s use `AlertDialog`, so
  Cancel and default buttons take the app tint; React Native's
  `showAlert` (UIAlertController) draws them in the label color.
- A single-line `TextInput` in body text sets
  `lineBreakModeIOS={singleLineBreakMode}` (`TextFieldRow` does): Fabric
  reuses native text input views, and a reused multiline view leaves a long
  placeholder clipped at a word with no ellipsis. A multiline `TextInput`
  reads its placeholder as its label while empty: set `accessibilityLabel`
  only when it has text, or VoiceOver reads the label twice.
- Text that wraps uses `WrappedText` (SwiftUI's wrapped line height once
  it wraps) or `BubbleText` in message bubbles. Inside a List or Form keep
  the default; elsewhere (scroll views, cards) pass `leading="standard"`.
- The Debug gallery shows every component and sample screens:
  `xcrun simctl openurl <udid> "xmtp-messenger-rn:///dev/gallery?section=components"`
  (sections: `components`, `settings`, `details`, `chats`, `chat`, `connect`).

## Session

- Screens use `@/session`: `useSessionState()`, `useActiveSession()` and the `sessionController` actions (`connect`, `retryRestore`, `signOut`, `deleteLocalData`, `continueReset`, `setAppearance`). `SessionProvider` starts the session and lends the open client to the SDK's `XmtpProvider`.
- An `ActiveSession` carries the client, `preferences`, `streams` (the one default message reader, plus conversation and event readers), `identities` and `ownHandles`. Register session-owned work in `session.scope`; sign out cancels and awaits it before `client.end()`. Drop async results unless `session.isCurrent()`.
- `app/_layout.tsx` mounts `SessionProvider` and `SessionRouter` (`src/features/startup`). The router owns the root screen: when the session state changes it pops the root stack and replaces it with the startup gate, Connect or the session's first screen (`src/core/sessionRoute.ts`). Screens never navigate to Connect or Chats after a sign-in or sign-out themselves. A launch deep link is pushed again on top of Chats; `/dev/*` routes are never redirected. Register every `app/dev` route in the root layout's `Stack.Protected guard={__DEV__}`, with a `routeTitles` title: a Release build then redirects a link to it to the startup gate, which the router leaves for the session's screen.
- The session machine and stream hub live in `src/core` behind ports, so `node --test` covers them; `src/session` wires the device and the SDK.
- Fast Refresh keeps the running session: reload the app after editing `src/session` or the core modules it uses. Running `install` (or `build-android`) under a live Metro removes the SDK package briefly; relaunch the app afterwards.
- `app/dev/session-probe.tsx` (Debug only) drives the session: `xcrun simctl openurl <udid> "xmtp-messenger-rn:///dev/session-probe?action=connect&url=http%3A%2F%2F127.0.0.1%3A5150"`. Other actions: `restore`, `retry`, `signOut`, `delete`, `continueReset`, `probe`, `handles`, `contend`, `allowRequests` (accepts every message request), `claim` (claims the handle in `name`), `createGroup` (a group named `name` with the comma-separated handles in `members`); add `&n=<new value>` to repeat one. Any app can open the URL scheme, so a link asks in the app before `signOut` and `delete` (the Settings alerts), `allowRequests`, `claim` and `createGroup`, and before `connect` and `retry` unless the app is signed out (`src/core/probeLinks.ts`); a UI check then taps the alert's action button. A link that launches the app waits until the session has started, and a link that arrives while another waits for confirmation is ignored. The probe's own buttons run at once.
- `app/dev/chat-probe.tsx` (Debug only) acts on the open chat, then returns to it: `xmtp-messenger-rn:///dev/chat-probe?action=<action>&c=<conversation>&m=<message>&n=<new value>`. Actions: `failNextSend` (the next send fails after it may have queued: Send outcome unknown), `failPublish` and `allowPublish` (publishing fails until allowed: the message stays Sending), `reply`, `retry`, `edit`, `delete`, `react` (`&e=<emoji>`). Without `m`, `retry` and `edit` use the newest own unpublished row.

## Chat

The chat stores are pure TypeScript in `src/core` (`chatTimeline.ts`, `chatComposer.ts`, `messageActions.ts`, with `chatHistory.ts` and `chatMessages.ts`) and are tested against the in-memory conversation in `chatFakes.ts`. `src/features/chat/chatRuntime.ts` wires them to the SDK (`sdkChatQueries.ts`), the send journal and the stream hub; views call its actions (`startReply`, `cancelReply`, `retry`, `edit`, `react`, `requestDelete`, `deleteMessage`, `copyText`, `submit`). A new content kind adds a `ChatContent` case, its SDK mapping in `chatContent` and a renderer in `MessageContent.tsx`. `MessageBubbleWrapper.tsx` delegates to `features/chat/interactions` (SwiftUI context menu with the quick-reaction preview, swipe to reply, VoiceOver actions, the send flight and the "Delete message?" alert); their rules are in `src/core/messageInteractions.ts`. `ComposerSlots.tsx` (attach menu, staged attachments) is a slot for later features. Settings renders `<SendReviewSection session={session} />` (`src/features/settings/sendReviewSlot.tsx`) directly in its `FormScreen`; the section brings its own `FormSection` and reloads on focus. Inside the context menu the bubble is hosted by SwiftUI without a width proposal, so `MessageInteractions` bounds it with the row's maximum bubble width. The SwiftUI host also takes no touches outside its frame, so on a row with reaction badges the hosted view grows 12 pt up to cover the badges' overhang. The quick-reaction buttons in the menu preview do not receive taps (the preview is one accessibility element); use the React item.

The Reactions sheet (`features/reactions`) and the emoji picker (`features/emoji`, catalog in `src/core/emojiData.ts`) draw their own headers. react-native-screens lays out a form sheet whose content root has a scroll view as a direct child as a scrolling sheet (warning "FormSheet with ScrollView expects at most 2 subviews"), and the list draws over the header: give the screen one `<View collapsable={false}>` root.

The request bar takes its Accept, Delete request and Block decisions from the same `RequestDecisionsStore` as Message requests (`useRequestDecisions`). Accept leaves Chats below the open chat and keeps it mounted.

## New message

`features/new-conversation` is a page sheet with its own stack: step 1 (`index`) and the review (`review`) share one `NewConversationStore` (`src/core/newConversation.ts`, recipient resolution and its tests) that `NewConversationSheet` owns in the sheet's layout (`useNewConversation()`). Create closes the sheet and pushes the new chat. `useScrollEdge()` (`scrollEdge.ts`) gives a form in a sheet SwiftUI's top edge: none at rest, the hard edge once content is under the bar. A static `scrollEdgeEffects: { top: "hard" }` draws the edge at rest too.

## Details

`src/core/details.ts` (`DetailsStore`: loads, ordered group writes, pending members and one poller) and `src/core/metadata.ts` (field rules and `MetadataStore`) are pure stores tested in `node --test`. `src/features/details/detailsStores.ts` gives Details, Edit group and Add member one shared store per conversation (`useSharedDetailsStore`), so errors and pending members show on Details after a sheet closes; Member, Permissions and Disappearing each own a store (`useOwnDetailsStore`). A sheet at a partial detent draws glass behind translucent cards: `useSheetFormProps()` (`sheetDetent.tsx`) returns the `FormScreen` props for the sheet's current detent. Block, Leave group and Remove from group confirm with `PopoverConfirmation` over the list.

## Attachments

The upload drafts and downloads are one pure store, `src/core/attachmentStore.ts` (with `attachments.ts` for models, copy, failure mapping and the saved drafts, and `sharedMedia.ts`), tested against `attachmentFakes.ts`. The recovery screen's chat choices are a separate pure store, `src/core/recoveryChats.ts`. `src/features/attachments` wires it to the SDK (`sdkAttachmentGateway.ts`) and the send journal: a send uploads first, then queues `sendRemoteAttachment` with the draft ID as the journal ID. Each open chat has one store (`chatAttachments.ts`), shared by the composer slots and the file cards in `MessageContent.tsx`. Picked files are copied into `<profile>/temp/attachment-staging/<uuid>` within the server limit and removed after `attachments.create`; Open and Save copy a verified file to `<profile>/exports/<uuid>/`, which the session removes when it ends. Image previews use a memory-only cache, so decrypted files stay in protected storage. Attachment recovery (`/attachment-recovery`, opened from Settings › Developer tools and from a staged file's "Review send") and Media and files (`/shared-media/<conversation>`, opened from Details) live in the same folder.

- `app/dev/attachment-probe.tsx` (Debug only): `xmtp-messenger-rn:///dev/attachment-probe?action=<action>&n=<new value>`. Actions: `limit` (`&bytes=<n>`: the upload limit a chat reads when it opens, for the "File exceeds the server upload limit" check), `clearLimit`, `holdUploads` (uploads wait, so the app can be stopped mid-upload) and `releaseUploads`.
- Launch with `XMTP_ALLOW_PRIVATE_ATTACHMENTS=1` for the local object store, and with `XMTP_UI_TEST_ATTACHMENT=<name>:<base64>` (Debug, at most 64 KiB) for the attach menu's "Test file".

## Native projects

`ios/` and `android/` are committed and edited by hand; nothing runs
`expo prebuild`, so `app.json` plugins and `expo-build-properties` do nothing.
After adding a native dependency, run `install` and commit `ios/Podfile.lock`
and the project changes. A new Swift file in `modules/messenger-platform`
also needs `install` before the next build, so the Pods project lists it.
Do not add `expo-sqlite` (the SDK links its own SQLCipher) or
`expo-dev-client`. Android keystores are never committed; builds sign with
AGP's default debug keystore.

## Tests

Don't write tests that reference other example app code, and don't write comments that reference features in other example apps.

These rules keep a valid change from breaking a test:

- Assert through what screens use: store state, the helpers screens call (`visibleSuggestions`, `statusPick`, `canJoin`), and the port calls whose presence is the behavior. Do not assert private fields, the identity of a state object, an order nothing shows, or leftover state on a closed store.
- Assert copy through its constant (`SessionCopy.signOutFailed`, `ConnectionCopy.reconnecting`), never as English text. For copy built from values, assert the values in it (the handle, the minimum, the dates) and that the branches differ. Copy lives in `copy.ts` or a `*Copy` object, not inline in `src/core` logic.
- Count fake calls only when the count is the behavior ("no SDK call after the chat closed", "saved once per position"). Do not assert exact call lists, call arguments, or the order of independent calls. For notifications, assert none or some, not an exact number.
- Inject a fault by content, not by call number: `failOnce((drafts) => drafts.some((entry) => entry.phase === "accepted"))`, not `failWrite = 2`.
- A tuning value (page size, threshold, limit, swipe distance, retry delay) comes from its named constant or from what the code reports, never a literal.
- A negative case must reach the guard it names: let the other guards pass, and assert what only that guard produces. Otherwise it fails, or goes silent, when an unrelated guard changes.
- Fakes resolve at once and apply their own effects. No timer that waits real time. Drain with `settle()` and hold work with `deferred()` from `src/core/testing.ts`; a zero-delay macrotask is fine. Close long-lived objects in `afterEach`. Record values inside a fake callback and assert after the call returns.
- Pin each saved format (index JSON, profile paths, preference file names, journal shape) with a literal fixture the RN app owns. A change to it must fail a test, because installed apps read it.
- Test a rule at one layer. Add a pure-function test only for cases the store test cannot reach cheaply, and put the variants of one rule in one table.
- Do not export a function, option or method only for a test. When a test is the last caller of production code, delete both.

Prove each new test fails under a named behavioral mutation, then restore it
and show it passes. Record the mutation in the change description.
