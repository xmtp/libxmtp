# XMTP Messenger for iOS

A SwiftUI example for iOS 26 and later. It uses this checkout's SDK.

## Run

Select Xcode 26 or later. Start the backend from the repository root:

`dev/nix-shell 'just backend up'`

Then run `nix run .#example-ios`, or `dev/nix-shell 'just example-ios start'`.
The launcher uses this worktree's backend URL. Set `XMTP_IOS_SIMULATOR` to a simulator name or UDID to select one. Pass `--headless` to launch without opening Simulator.

## Code structure

ExampleApp contains SwiftUI routes and platform storage. ExampleCore contains values, policies and stores. ExampleSDK maps the local SDK to Core protocols. The app uses one session scope and drops asynchronous completions from replaced sessions or destinations.

## Connect

Join a server with a loopback HTTP or remote HTTPS URL. Access tokens appear only when the server requires authentication. Accounts and encrypted messages remain on this device after sign-out. Delete local data removes the account from this device and can resume an interrupted removal.

## Handles

Choose a handle after joining, or skip and choose one in Settings. New message accepts handles, Ethereum addresses and inbox IDs. Current handles identify people throughout the app.

## Local storage

Secrets are stored in this app's device-only Keychain. Each profile has an excluded-from-backup directory protected until first unlock. Preferences use AES-GCM and atomic writes. Sign-out removes credentials and exported files while retaining account keys and messages.

## Optional notifications

Debug and Release builds leave APNs off. A signed device build needs a development team in gitignored Config/Local.xcconfig, release SDK bindings and an APNs-configured backend.

## Nix

Nix supplies SDK bindings and tools; Xcode builds as the invoking user into .build/DerivedData. `just example-ios build`, CI and the launcher use dev/build. Device builds require `sdks/ios/dev/bindings --release` and signing.

## Manual checks

- Compare each screen to the approved design in light and dark appearance.
- Check VoiceOver, AX5 text, Increase Contrast, Reduce Transparency and Reduce Motion.
- Verify join, token-required join, failed restore retry and interrupted reset recovery.
- Change a handle and switch back to another owned handle through confirmation.
- Create a direct message and a group with handles and inbox IDs.
- Confirm the launcher opens the correct worktree's backend.

The host tests and simulator journeys are run through the recipes in AGENTS.md.

### Local attachment networking

Private and loopback attachment destinations are blocked by default, including when the backend is local. For a trusted local development backend only, launch with `XMTP_ALLOW_PRIVATE_ATTACHMENTS=1`. The value must be exactly `1`, and the backend must be loopback. This opts into the SDK permission for **all private and loopback attachment destinations**, not just the backend origin. Do not enable it for untrusted conversations. The local J4 file journey opts in explicitly.
