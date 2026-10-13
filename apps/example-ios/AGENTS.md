# iOS example

Run from the repository root in the iOS Nix shell:

- `dev/nix-shell 'just example-ios build'` builds the app and journeys.
- `dev/nix-shell 'just example-ios test'` runs host unit tests.
- `dev/nix-shell 'just example-ios test-ui'` runs simulator journeys.
- `dev/nix-shell 'just example-ios lint'` checks Swift and dependency boundaries.
- `dev/nix-shell 'just example-ios format'` formats sources.
- `dev/nix-shell 'just example-ios start'` builds and launches.

Only ExampleSDK imports XmtpSdk. ExampleCore imports Foundation, Observation and CryptoKit only. Views read store state and send actions. Every asynchronous completion must pass its session and destination admission guard before changing state. Cancel and await session tasks before ending the SDK client. Never use Task.detached.

Prefer system components and native glass styles. Keep copy in each feature's string catalog. Use previews for light, dark and accessibility sizes. Unit tests cover safety policies; simulator journeys cover the main flows. Prove each new test fails under a named mutation before restoring it.
