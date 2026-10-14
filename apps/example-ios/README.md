# XMTP Messenger for iOS

A SwiftUI foundation scaffold for iOS 26 and later. It uses this checkout's SDK.

This revision launches the Join a server scaffold. Accounts, handles, conversations and the storage behavior below are planned follow-up work; they are not implemented by this foundation revision.

## Run

Select Xcode 26 or later. Start the backend from the repository root:

`dev/nix-shell 'just backend up'`

Then run `nix run .#example-ios`, or `dev/nix-shell 'just example-ios start'`.
The launcher uses this worktree's backend URL. Set `XMTP_IOS_SIMULATOR` to a simulator name or UDID to select one. Pass `--headless` to launch without opening Simulator.

## Code structure

ExampleApp contains the Join a server scaffold, route placeholders, design tokens and assets. ExampleCore contains RouteOwner, SessionFence, session and destination identifiers, and the FeatureStore interface. ExampleSDK contains only a module marker. Platform storage, feature stores, SDK adapters and a session task scope are planned follow-up work.

## Connect (planned)

Join a server with a loopback HTTP or remote HTTPS URL. Access tokens appear only when the server requires authentication. Accounts and encrypted messages remain on this device after sign-out. Delete local data removes the account from this device and can resume an interrupted removal.

## Handles (planned)

Choose a handle after joining, or skip and choose one in Settings. New message accepts handles, Ethereum addresses and inbox IDs. Current handles identify people throughout the app.

## Local storage (planned)

Secrets are stored in this app's device-only Keychain. Each profile has an excluded-from-backup directory protected until first unlock. Preferences use AES-GCM and atomic writes. Sign-out removes credentials and exported files while retaining account keys and messages.

## Optional notifications (planned)

Debug and Release builds leave APNs off. A signed device build needs a development team in gitignored Config/Local.xcconfig, release SDK bindings and an APNs-configured backend.

## Nix

Nix supplies SDK bindings and tools; Xcode builds as the invoking user into .build/DerivedData. `just example-ios build`, CI and the launcher use dev/build. Device builds require `sdks/ios/dev/bindings --release` and signing.

## Manual checks for the planned full app

- Compare each screen to the approved design in light and dark appearance.
- Check VoiceOver, AX5 text, Increase Contrast, Reduce Transparency and Reduce Motion.
- Verify join, token-required join, failed restore retry and interrupted reset recovery.
- Change a handle and switch back to another owned handle through confirmation.
- Create a direct message and a group with handles and inbox IDs.
- Confirm the launcher opens the correct worktree's backend.

The recipes in AGENTS.md run the foundation host tests and a simulator smoke test that checks the Join a server scaffold. Feature journeys are planned follow-up work.
