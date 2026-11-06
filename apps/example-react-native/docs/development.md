# Local development and session probes

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
x86_64-linux output built with the pinned Xcode: a Mac only substitutes it
from Cachix. When this checkout's SDK identity is not cached (for example after a
root `Cargo.toml` edit, which the identity covers), `sdk` fails fast. Use the
package of a merged revision whose SDK content matches:
`XMTP_RN_SDK_REV=<rev> dev/nix-shell 'just example-react-native sdk'`.

## Session

- Screens use `@/session`: `useSessionState()`, `useActiveSession()` and the `sessionController` actions (`connect`, `retryRestore`, `signOut`, `deleteLocalData`, `continueReset`, `setAppearance`). `SessionProvider` starts the session and lends the open client to the SDK's `XmtpProvider`.
- An `ActiveSession` carries the client, `preferences`, `streams` (the one default message reader, plus conversation and event readers), `identities` and `ownHandles`. Register session-owned work in `session.scope`; sign out cancels and awaits it before `client.end()`. Drop async results unless `session.isCurrent()`.
- `app/_layout.tsx` mounts `SessionProvider` and `SessionRouter` (`src/features/startup`). The router owns the root screen: when the session state changes it pops the root stack and replaces it with the startup gate, Connect or the session's first screen (`src/core/sessionRoute.ts`). Screens never navigate to Connect or Chats after a sign-in or sign-out themselves. A launch deep link is pushed again on top of Chats; `/dev/*` routes are never redirected. Register every `app/dev` route in the root layout's `Stack.Protected guard={__DEV__}`, with a `routeTitles` title: a Release build then redirects a link to it to the startup gate, which the router leaves for the session's screen.
- The session machine and stream hub live in `src/core` behind ports, so `node --test` covers them; `src/session` wires the device and the SDK.
- Fast Refresh keeps the running session: reload the app after editing `src/session` or the core modules it uses. Running `install` (or `build-android`) under a live Metro removes the SDK package briefly; relaunch the app afterwards.
- `app/dev/session-probe.tsx` (Debug only) drives the session: `xcrun simctl openurl <udid> "xmtp-messenger-rn:///dev/session-probe?action=connect&url=http%3A%2F%2F127.0.0.1%3A5150"`. Other actions: `restore`, `retry`, `signOut`, `delete`, `continueReset`, `probe`, `handles`, `contend`, `allowRequests` (accepts every message request), `claim` (claims the handle in `name`), `createGroup` (a group named `name` with the comma-separated handles in `members`); add `&n=<new value>` to repeat one. Any app can open the URL scheme, so a link asks in the app before `signOut` and `delete` (the Settings alerts), `allowRequests`, `claim`, `createGroup`, `connect` and `retry` (`src/core/probeLinks.ts`). It asks also when the app is signed out, because a `connect` link would open a session on the server it names; that alert says the app is signed out. A UI check then taps the alert's action button. A link waits while the session starts or opens (`starting`, `opening`) and runs once the state settles, so a link that launches the app with a saved profile waits until that session is active; a newer link replaces one that waits. A link that arrives while another waits for confirmation is ignored. The probe's own buttons run at once.
- `app/dev/chat-probe.tsx` (Debug only) acts on the open chat, then returns to it: `xmtp-messenger-rn:///dev/chat-probe?action=<action>&c=<conversation>&m=<message>&n=<new value>`. Actions: `failNextSend` (the next send fails after it may have queued: Send outcome unknown), `failPublish` and `allowPublish` (publishing fails until allowed: the message stays Sending), `reply`, `retry`, `edit`, `delete`, `react` (`&e=<emoji>`). Without `m`, `retry` and `edit` use the newest own unpublished row. A link that changes data asks in the app first: `retry`, `react` and `delete` (with the chat's "Delete message?" alert text), and `reply` and `edit`, which replace the saved draft (`src/core/probeLinks.ts`); a UI check then taps the alert's action button, and the probe removes its own screen when the alert closes, also when a link to another screen covered it. The send faults run at once, and a chat probe link that arrives while another waits for confirmation is ignored.

## Native projects

`ios/` and `android/` are committed and edited by hand; nothing runs
`expo prebuild`, so `app.json` plugins and `expo-build-properties` do nothing.
After adding a native dependency, run `install` and commit `ios/Podfile.lock`
and the project changes. A new Swift file in `modules/messenger-platform`
also needs `install` before the next build, so the Pods project lists it.
Do not add `expo-sqlite` (the SDK links its own SQLCipher) or
`expo-dev-client`. Android keystores are never committed; builds sign with
AGP's default debug keystore.
