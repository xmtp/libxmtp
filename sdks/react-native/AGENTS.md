# React Native SDK

Run every command from the repository root through `dev/nix-shell`.
Use the `writing-typescript` skill for JavaScript and TypeScript changes.

- `dev/nix-shell 'just sdk generate react-native'` renders the full native public API into `target/sdk-generated/typescript-react-native`. The raw binding is private.
- `dev/nix-shell 'just react-native check'` builds JS/types and checks package source/types. JS/types need no mobile binary build.
- `dev/nix-shell 'just react-native components android target/rn-release/android'` exports Android components on Linux. Replace `android` with `ios` on Darwin. Exchange completed export directories between the hosts.
- `dev/nix-shell 'just react-native package target/rn-release/android target/rn-release/ios'` imports matched components and realizes the canonical tarball on Linux, then exports `target/rn-release/package`.
- `dev/nix-shell 'just react-native load-package target/rn-release/package'` imports that exact package closure on a consumer host.
- `dev/nix-shell 'just react-native test-installed android all'` runs installed bare/Expo consumers. Use `ios` on Darwin. Native consumers must install the canonical tarball, enable Hermes and New Architecture, and must not build Rust.
- `dev/nix-shell 'just react-native test-native'` compiles production native lifecycle and owner helpers against a controlled ABI. It complements the installed Hermes proofs.
- `dev/nix-shell 'just react-native test-react'` stages Node's sibling public classes and runs the React ownership tests without a simulator. They own the React proofs. The installed Hermes run checks only that the `/react` entry resolves.
- `dev/nix-shell 'just react-native test-control'` runs the real HTTP reload-settlement and final-result admission tests without a simulator or SDK build.
- `dev/nix-shell 'just react-native check-package'` checks package admission, binary completeness, and receipt rejection using local fixtures, and which phases `XMTP_RN_JOURNEYS` selects. It does not prove native execution.

The private `src/specs/NativeXmtpReactNative.ts` and `codegenConfig` ship in the npm product for React Native autolinking/codegen. They do not add a public SDK export. Keep them matched to the Java/Objective-C++ provider and include their hashes in the product receipt.

The npm product ships matched prebuilt Rust libraries, generated C++, native adapters, and the runtime from the committed ubrn fork. Keep these inputs in the receipt. Do not replace the fork runtime with an npm release that merely shares its version number.

`src/react` is optional. It imports the public root and never creates a second native runtime. Expose `/react` only when its compiled entry and types are present. Internal consumers are test fixtures. The maintained public example is `apps/example-react-native`; it installs the SDK package the same way an app does.

RN staging always checks generated source/render identities. Declared Nix inputs supply the paired private build values `XMTP_SDK_EXPECTED_SOURCE` and `XMTP_SDK_EXPECTED_RENDER`; other routes compare with the current checkout. An explicit generated directory does not bypass admission. Generated, runtime, and checkout fork revisions must agree.
