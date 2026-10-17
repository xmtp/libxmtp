# @xmtp/react-native-sdk

Version 8 uses the generated native XMTP API shared with the Node SDK. It uses Hermes and the React Native New Architecture on iOS and Android. Expo apps use a development build with the same native module; Expo Go cannot load it.

The npm artifact includes prebuilt Rust libraries. Apps need the normal React Native native build tools. They do not need Rust or Nix. Rebuild the native app after installing or upgrading the package. Autolinking adds the XMTP module.

The product targets iOS arm64 devices and arm64 simulators, and Android arm64-v8a, armeabi-v7a, x86, and x86_64. The native deployment floors are iOS 15.1 and Android API 24; the Expo consumer floor is iOS 16.4. The internal required consumers pin React Native 0.88.0 and Expo SDK 58. Candidate versions become supported only after installed-package native checks pass; source configuration alone does not establish support.

Apps supply the backend, signer, persistent database encryption key, push token, permissions, and media sources. The SDK exposes native storage, file archives, integrated attachments, and push registration through the shared public API. End a client before deleting its database. An app must arrange native background and cold-start handlers; JavaScript cannot run while the operating system has suspended it.

All pre-8 consumers must migrate their API calls. Legacy data moves through the shared closed-database-to-archive converter followed by normal archive import. This is not automatic database adoption and does not preserve installation keys or network identity. See the sibling converter's schema compatibility and source-preservation rules in `crates/xmtp_legacy_migration/README.md`.

Follow the [React Native setup guide](https://docs.xmtp.org/get-started/react-native/) for app integration.

Repository build and proof commands are in [AGENTS.md](https://github.com/xmtp/libxmtp/blob/main/sdks/react-native/AGENTS.md). The canonical package is assembled from matched Linux Android and Darwin iOS components. Its receipt records every shipped asset and the source, binding, and runtime identities.
