# iOS example

Run from the repository root. Recipes select the iOS Nix shell.

- `dev/nix-shell 'just example-ios build'` builds the simulator app and journeys.
- `dev/nix-shell 'just example-ios test'` prepares local SDK bindings and runs host unit tests.
- `dev/nix-shell 'just example-ios test-ui'` runs the five simulator journeys against the worktree backend.
- `dev/nix-shell 'just example-ios lint'` checks Swift and dependency boundaries.
- `dev/nix-shell 'just example-ios format'` formats app and package sources.
- `dev/nix-shell 'just example-ios start'` builds and launches; append `--headless` to leave Simulator.app closed.
- `CONFIGURATION=Push dev/nix-shell 'just example-ios build'` compiles optional APNs integration for the simulator. It does not prove device delivery.

Only ExampleSDK and the UI-test runner import XmtpSdk. ExampleCore imports Foundation, Observation and CryptoKit only. One store owns each route and exposes private-set state plus send actions. Views read that state and send actions. Every asynchronous result passes both SessionKey and destination-token admission before changing state. Recreate route stores when the destination token changes; never weaken admission to preserve a stale store.

Cancel and await session tasks and store cleanup before ending the SDK client. Never use Task.detached. A background push restores only an existing signed-in profile with its database and secrets, without starting UI collectors. Registration is serialized with session shutdown. Privacy writes bracket push admission; the 25-second background deadline revokes late results and completes once.

Use system components before custom drawing. Primary buttons use native glassProminent, secondary and destructive buttons use glass. Custom chrome uses glassEffect through the design helpers. Keep copy in feature string catalogs and keep design tokens in DesignSystem. Motion follows the reference path and sequence, but production timing is fast; Reduce Motion uses 0.2-second cross-fades.

Add light, dark and AX3 previews for screen states. Check real simulator pixels against the reference, including AX5, VoiceOver and iPad. Do not mark a manual or device check passed from source inspection, a preview or an APNs simulator compile.

The build, focused host tests and five backend journeys are the test layers. Prove each new test fails under a named behavioral mutation, then restore it and show it passes. Record the mutation and manual-check results in the change description.
