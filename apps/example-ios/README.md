# XMTP Messenger for iOS

A SwiftUI example for iOS 26 and later, built against the SDK in this checkout. The supported development target is the simulator.

For team distribution, follow the [TestFlight guide](TESTFLIGHT.md).

## Code structure

- `ExampleApp` owns SwiftUI views, native glass controls, protected local files and APNs integration.
- `Packages/ExampleKit/Sources/ExampleCore` owns values, policies and feature stores. It has no UI or SDK imports.
- `Packages/ExampleKit/Sources/ExampleSDK` maps the local SDK to Core protocols. Only this layer and the UI-test runner import `XmtpSdk`.
- Stores expose `state` and `send`. Session generations and destination tokens reject late results. Session tasks are cancelled and awaited before the SDK client ends.
- Host tests cover safety policies and SDK mappings. Five simulator journeys use a real backend. Previews and manual checks cover visual and platform behavior.

## Connect

Select Xcode 26 or later. Run from the repository root:

```sh
dev/nix-shell 'just backend up'
nix run .#example-ios
```

The launcher loads this worktree’s backend address. Set `XMTP_BACKEND_URL` to use another server. Remote servers require HTTPS; HTTP is allowed only for loopback. The Access token field appears only when the current server requires authentication.

Joining the same normalized URL reuses the profile and inbox. Sign out ends the session and removes its access token and exports; identity keys and messages remain. Delete local data removes this server’s local profile and files and can resume after interruption. It does not delete messages on other devices.

Secrets use device-only Keychain items. Profile files are excluded from backup and protected until first unlock. Preferences use AES-GCM and atomic writes.

## Handles

Choose a handle after joining, or skip and choose one in Settings. Claims require an explicit choice. Change handle confirms every claim, including switching back to another owned handle. Server rules decide whether the claim succeeds.

New message and Add member accept handles with or without `@`, Ethereum addresses and inbox IDs. Inputs resolve before SDK writes. People show their current handle when available, otherwise a shortened inbox ID.

## Chat

Chats contains allowed conversations. Unknown senders appear in Message requests; blocked conversations appear in neither. Accept a request before replying or downloading files. Message actions include reply, reactions, copy and deletion of your own published messages. Failed sends offer Retry and Edit. Swipe horizontally to reply.

The composer is saved before sending. Accepted message IDs are saved before clearing it, and retries publish the same ID. After an interrupted send with an unknown outcome, review the conversation in Settings before clearing saved status. Clearing status does not delete, cancel or resend a message.

Attachments require server support. Photos and Files are copied into protected storage while enforcing the upload limit. Downloads require a user action and verification before opening or saving. Attachment recovery lists interrupted or unassigned drafts.

## Conversation fields

Details contains local names, members, permissions, disappearing messages, media and developer tools. A local name stays on this device. Permission presets perform ordered SDK writes and read back policies after failure. Only super admins can change permission policies; the SDK enforces that authority.

Disappearing messages offers Off, 1 hour, 8 hours, 1 day, 1 week and Custom seconds. Custom 0 means off. Values that overflow the nanosecond representation are rejected before writing.

Group fields and My fields follow the committed metadata registry. The Android metadata fixture can supply sample fields:

```sh
dev/nix-shell "python3 apps/example-android/fixtures/metadata_backend.py --backend /path/to/xmtp-backend -- bash -c 'XMTP_BACKEND_URL=\"\$XMTP_METADATA_BACKEND_URL\" just example-ios start'"
```

Replace `/path/to/xmtp-backend` with a backend executable from this checkout. The fixture loads `metadata-catalogue.toml`, starts a temporary database and publishes an `http://127.0.0.1:<port>` URL. The command above passes that URL to the simulator launcher. It needs the shared PostgreSQL and S3 services from `just backend up`.

## Local history limits

Lists and history load 50 rows at a time. Search matches loaded rows only. The transcript cache keeps up to three conversations with 500 rows each. Media and files shows the latest 50 attachment messages. Unread counts use local insertion time, so imported messages and equal timestamps can affect them.

## Optional notifications

Debug and Release do not request notification permission or an APNs token. Settings shows **Off: APNs is not configured**. The optional `Push` configuration asks permission after notifications are turned on. Conversation Mute and Unmute controls exist only in that configuration.

Registration includes allowed and unknown conversations and welcomes, but excludes sync groups and commits. Silent pushes are checked against the current profile, installation, conversation state and explicit mute preference. Local alerts contain only **XMTP Messenger** and **You got a message.** Taps recheck ownership and privacy before navigation.

Accepted notifications keep an encrypted replay window of 512 global sequence IDs and a retired sequence floor. Older notifications below that floor stay suppressed, including very late out-of-order arrivals; message sync is unaffected. Pending posts live only in memory. A receipt is saved after iOS accepts the alert, and existing pending or delivered iOS identifiers are checked before posting again. iOS acceptance and local persistence are not atomic, so a crash followed by removal of the system notification can still permit a replay.

Real APNs delivery needs a physical device, an Apple development team and a backend with `[push.apns]`. The local backend configures HTTP push only. A simulator compile does not prove APNs delivery.

Put `DEVELOPMENT_TEAM = <your team ID>` in gitignored `Config/Local.xcconfig`. Generate device SDK slices and build with Xcode, or run:

```sh
NIX_DEVSHELL=ios dev/nix-shell 'sdks/ios/dev/bindings --release'
NIX_DEVSHELL=ios dev/nix-shell 'env -u LD xcodebuild -project apps/example-ios/ExampleApp.xcodeproj -scheme ExampleApp -configuration Push -sdk iphoneos -destination "generic/platform=iOS" IPHONEOS_DEPLOYMENT_TARGET=26.0'
```

APNs keys and tokens never belong in the repository or logs. If permission is denied, use **Open Settings** in the Notifications section.

## Nix

Nix supplies tools, generated Swift and the framework. Xcode builds as the invoking user in `.build/DerivedData`. The launcher, recipes and CI share `dev/build` and `dev/start`.

```sh
dev/nix-shell 'just example-ios build'
dev/nix-shell 'just example-ios test'
dev/nix-shell 'just example-ios test-ui'
dev/nix-shell 'just example-ios lint'
dev/nix-shell 'just example-ios start --headless'
```

Set `XMTP_IOS_SIMULATOR` to an installed name or UDID. Otherwise the launcher selects an available iPhone running iOS 26 or later. `--headless` leaves Simulator.app closed. Simulator builds use ad-hoc signing. The launcher does not start the backend.

## Manual checks

Run these after the five simulator journeys and after related changes. Record actual results, device/runtime and evidence in the change description. Missing devices, credentials or fixtures are **not run**, not passes. P identifiers name implementation requirements.

| Check | Expected result | Requirements |
| --- | --- | --- |
| Every route and state in light and dark | Compare actual simulator screenshots with the canvas: layout, spacing, hierarchy, colors and native glass. | P45, P105 |
| Join, New message, send, react, attach, accept | Record six transitions. Preserve path and sequence, with much faster production timing and prompt interaction. | P106 |
| Reduce Motion | Short cross-fades replace all six morphs and flights. | P102, P106 |
| VoiceOver, AX5, Bold Text, Increase Contrast, Reduce Transparency | No clipped primary actions; icon labels and message actions work; content remains readable. | P101 |
| iPad portrait and landscape | Chats sidebar, highlighted selection and selected route in detail. | P100 |
| Token-required server | Field follows current server; Join waits for required input. | P25–P27 |
| Failed restore | Cannot restore this account. Try again. with Retry. | P31 |
| Interrupted reset after database removal | Only Continue local data removal appears; finishing removes the profile and secrets. | P33, P34 |
| Change handle and switch back | Each claim confirms; Settings shows the result. | P44 |
| Settings and appearance | Profile, Server, Notifications, Appearance, Privacy, Send needs review, Developer tools, This device; appearance survives relaunch. | P95 |
| More than 120 messages; delete saved anchor | No duplicate history rows; reopening shows Position changed. | P58–P60, P70 |
| Read position | Background, an older page and a noncurrent chat do not advance markers. | P53 |
| Stop and restart backend | Reconnecting appears, then disappears. | P54 |
| Force quit during send | Unknown outcome locks composer and appears in Send needs review. Clearing status does not resend. | P67, P68 |
| Swipe reply; failed-message Retry and Edit | Haptic at 48 pt; Retry reuses ID; Edit copies text. | P66, P73, P74 |
| Photo and Files pickers; oversized file | Both stage/send; upload limit enforced while copying. | P76, P77 |
| Server without attachments | Controls hidden; use Android unsupported-backend fixture with shared Docker stack. | P76 |
| Interrupted upload | Recovery offers review, assignment and discard as appropriate. | P78, P84 |
| Verified media | Download, verify, open, save; sign out and reset remove exports. | P80–P83, P104 |
| Metadata fixture with two members | Set string, map entry and own user field; second member reads them. | P94 |
| Disappearing and leave | 1 hour, Custom 0, reject 9223372037 seconds without a write; pending leave. | P92, P93 |
| Signed Push build | Actual generic APNs alert; mute/block/sign-out and old-profile taps enforce privacy. | P97–P99 |
| Push with local backend | Token-bearing signed build reports Backend APNs channel is not configured. | P97 |
| Both launchers | Correct worktree URL; visible launch opens Simulator; headless succeeds without opening it. | P8, P12–P14 |

### Local attachment networking

Private and loopback attachment destinations are blocked by default, including when the backend is local. For a trusted local development backend only, launch with `XMTP_ALLOW_PRIVATE_ATTACHMENTS=1`. The value must be exactly `1`, and the backend must be loopback. This opts into the SDK permission for **all private and loopback attachment destinations**, not just the backend origin. Do not enable it for untrusted conversations. The local J4 file journey opts in explicitly.
