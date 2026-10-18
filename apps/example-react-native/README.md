# XMTP Messenger for React Native

A React Native example for iOS 26 and later and Android, built with Expo
(bare workflow) against the React Native SDK package. On iOS it follows the
SwiftUI example in [`apps/example-ios`](../example-ios). The supported
development targets are the iOS simulator and the Android emulator.

This is the scaffold. Later changes add the screens.

## Code structure

- `app/` holds the expo-router routes. Each route renders a feature screen.
- `src/core/` holds pure TypeScript values and policies, tested with `node --test`.
- `src/platform/` wraps Expo modules and the local module in `modules/messenger-platform` (launch environment, file protection, backup exclusion, navigation subtitle).
- `ios/` and `android/` are committed native projects from the Expo bare template, edited by hand.

## Run locally

Run from the repository root. Start the backend, link the SDK package, then
start Metro and the app in separate terminals:

```sh
dev/nix-shell 'just backend up'
dev/nix-shell 'just example-react-native sdk'
dev/nix-shell 'just example-react-native metro'
dev/nix-shell 'just example-react-native start-ios'
```

`start-ios` builds, installs and launches on the first available iPhone
simulator. The app uses this worktree's backend; set `XMTP_BACKEND_URL` to
use another server. `CONFIGURATION=Release` builds an app with an embedded
bundle that needs no Metro. For Android, run
`dev/nix-shell 'just example-react-native start-android'` while Metro runs.

Check the JavaScript before a change:

```sh
dev/nix-shell 'just example-react-native check'
```
