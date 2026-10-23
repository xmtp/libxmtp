# React Native example

`dev/nix-shell 'just example-react-native check'` checks types, lint, format, and
unit tests without mobile builds. Recipes select `react-native-local`.
Run/install commands: [README](README.md#run-locally).

- `app/` holds route adapters only, with no platform suffixes. `src/core/` is pure
  TypeScript: no React/Expo/SDK runtime imports; explicit `.ts` imports for Node tests.
  Use `@/` for app imports outside core.
- Screens use `@/session`; register owned work in `session.scope` and drop results
  unless `session.isCurrent()`. Cancel/await work before `client.end()`.
  SessionRouter owns sign-in/out navigation; screens do not navigate it themselves.
- Keep copy in feature `copy.ts` / `src/copy`; keep test IDs and accessibility labels
  stable. Use `src/ui` components and tokens. For UI/store changes read
  [UI conventions](docs/ui.md); for tests read [testing conventions](docs/testing.md).
- Hermes lacks `crypto.getRandomValues`, `TextDecoder`, and `Buffer`. Pass native
  paths, not `file://` URIs. Persist SDK `bigint` values as decimal strings.
- Log errors with `errorLogText`, never raw errors/full IDs; use at most 8 ID characters.
- Native projects are hand-maintained; do not run `expo prebuild` or add
  `expo-sqlite`/`expo-dev-client`. Native dependency/new Swift-file changes need
  `install`; commit project changes and `Podfile.lock`, never Android keystores.
- Read [development and probes](docs/development.md) for parallel Metro/simulator
  setup, SDK linking, launch settings, and session work. Debug routes stay behind
  `Stack.Protected`; mutating probe links require in-app confirmation. Android
  Release must ignore launch extras and never allow private attachment hosts.
