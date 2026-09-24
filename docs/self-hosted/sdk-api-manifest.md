# SDK API manifest

This manifest classifies the public exports of the four current SDKs before they move to the Rust facade. The [design Ref](https://plan.ref.tools/eG4NJ6emCjsHcWH0), especially Sections 11 and 19, is the authority. The implementation plan adopts Section 20 items 1, 2, 3, 4, and 7, and uses `end()` for async shutdown. Final names use stock generator spelling: `ID` suffixes, `unsafe` camel case, string IDs, and one `Timestamp` value with `.ns` and `.date`.

`generated` means the facade generator emits the API. `static runtime` means hand-written host code ships with generated output. `platform helper` means native OS code stays in the SDK. `alias` means a deprecated compatibility name. `approved removal` means the current export leaves the API. A dash in Final name marks a removal. Each table has one row per inventory entry; a generated Swift source-family row covers the stated number of declarations. A source path and line number distinguish overloads. Kind names the current declaration form. The helper counts source-declared Swift public/open items, Kotlin public declarations and constructor properties, and TypeScript package exports plus exported class members. Compiler-synthesized members are outside this source inventory. The counts are declaration counts, not table-row counts. Run `python3 dev/sdk/inventory.py --check` to recompute them.

| SDK | Public declarations |
| --- | ---: |
| Swift | 7138 |
| Kotlin | 922 |
| Node | 472 |
| Browser | 482 |

## Swift

| Current export | Kind | Final name | Status | Design ref | Notes |
| --- | --- | --- | --- | --- | --- |
| `sdks/ios/Sources/XMTPiOS/Auth.swift:4 Credential` | struct | `Credential` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Auth.swift:6 name` | let | `name` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Auth.swift:8 value` | let | `value` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Auth.swift:10 expiresAtSeconds` | let | `expiresAtSeconds` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Auth.swift:12 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Auth.swift:19 description` | var | `description` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Auth.swift:23 debugDescription` | var | `debugDescription` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Auth.swift:34 AuthCallback` | typealias | `AuthCallback` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:4 PreEventCallback` | typealias | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Client.swift:5 MessageMetadata` | typealias | `MessageMetadata` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:7 ClientError` | enum | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Client.swift:8 ClientError.creationError` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Client.swift:9 ClientError.missingInboxId` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Client.swift:10 ClientError.invalidInboxId` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Client.swift:12 description` | var | `description` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:23 errorDescription` | var | `errorDescription` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:28 ForkRecoveryPolicy` | enum | `ForkRecoveryPolicy` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:29 ForkRecoveryPolicy.none` | case | `ForkRecoveryPolicy.none` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:30 ForkRecoveryPolicy.allowlistedGroups` | case | `ForkRecoveryPolicy.allowlistedGroups` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:31 ForkRecoveryPolicy.all` | case | `ForkRecoveryPolicy.all` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:45 ForkRecoveryOptions` | struct | `ForkRecoveryOptions` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:46 enableRecoveryRequests` | var | `enableRecoveryRequests` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:47 groupsToRequestRecovery` | var | `groupsToRequestRecovery` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:48 disableRecoveryResponses` | var | `disableRecoveryResponses` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:49 workerIntervalNs` | var | `workerIntervalNs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:51 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:73 VisibilityConfirmationOptions` | struct | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Client.swift:74 timeoutMs` | var | `timeoutMs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:76 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:89 DbPoolOptions` | struct | `DbPoolOptions` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:90 maxPoolSize` | var | `maxPoolSize` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:91 minPoolSize` | var | `minPoolSize` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:93 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:100 ClientOptions` | struct | `ClientOptions` | static runtime | 11.4 Swift | Host options wrapper accepts codecs or callbacks (11.1, 20.1). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:102 Api` | struct | `BackendOptions` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:104 backendUrl` | var | `backend.url` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:106 env` | var | `storage.label` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:107 appVersion` | var | `appVersion` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:109 authCallback` | var | `backend.credentials` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:111 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:122 api` | var | `backend` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:123 codecs` | var | `codecs` | static runtime | 11.4 Swift | Host options wrapper accepts codecs or callbacks (11.1, 20.1). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:127 preAuthenticateToInboxCallback` | var | `handlers.preAuthenticate` | static runtime | 11.4 Swift | Host options wrapper accepts codecs or callbacks (11.1, 20.1). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:129 dbEncryptionKey` | var | `storage.encryptionKey` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:130 dbDirectory` | var | `storage.location.directory` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:131 deviceSyncEnabled` | var | `deviceSync` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:132 debugEventsEnabled` | var | — | approved removal | 11.4 Swift | Debug events or unstable callbacks leave ClientOptions (11.4, 19.31/35). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:133 forkRecoveryOptions` | var | `forkRecovery` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:134 waitForRegistrationVisible` | var | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Client.swift:135 dbPoolOptions` | var | `storage.pool` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:138 unstableChangeCallbacks` | var | — | approved removal | 11.4 Swift | Debug events or unstable callbacks leave ClientOptions (11.4, 19.31/35). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:140 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:188 InboxId` | typealias | `InboxID` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:190 Client` | class | `Client` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:192 enableNotifications` | func | `enableNotifications` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:201 disableNotifications` | func | `disableNotifications` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:210 notificationState` | func | `notificationState` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:216 inMemoryDbPath` | let | `inMemoryDbPath` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:226 manageStreamLifecycle` | var | `manageStreamLifecycle` | platform helper | 2 | Native lifecycle or log writer helper (2, 19.26/35). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:228 inboxID` | let | `inboxID` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:229 libXMTPVersion` | let | `libxmtpVersion` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:230 dbPath` | let | `storage.path` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:231 installationID` | let | `installationID` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:232 publicIdentity` | let | `identity` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:233 environment` | let | `options.storage.label` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:241 isInMemory` | var | `isInMemory` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:245 conversations` | var | `conversations` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:251 preferences` | var | `preferences` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:255 debugInformation` | var | `diagnostics` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:261 register` | func | — | approved removal | 11.4 Swift | Global codec registration becomes ClientOptions.codecs (11.4, 19.9). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:350 create` | func | `create` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:383 createInMemory` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Client.swift:402 build` | func | `build` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:434 ffiCreateClient` | func | `Client.build()` | alias | 11.4 Swift; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:599 connectToApiBackend` | func | `Backend.connect()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:633 getOrCreateInboxId` | func | `Client.inboxID()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:645 revokeInstallations` | func | `revokeInstallations` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:688 ffiApplySignatureRequest` | func | `unsafeApplySignatureRequest()` | alias | 11.4 Swift; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:710 ffiRevokeInstallations` | func | `unsafeRevokeInstallationsSignatureRequest()` | alias | 11.4 Swift; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:763 canMessage` | func | `canMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:777 inboxStatesForInboxIds` | func | `inboxStatesForInboxIDs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:788 keyPackageStatusesForInstallationIds` | func | `keyPackageStatusesForInstallationIDs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:807 getNewestMessageMetadata` | func | `getNewestMessageMetadata` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:848 addAccount` | func | `addAccount` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:880 removeAccount` | func | `removeAccount` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:899 revokeAllOtherInstallations` | func | `revokeAllOtherInstallations` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:917 revokeInstallations` | func | `revokeInstallations` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:937 canMessage` | func | `canMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:944 canMessage` | func | `canMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:957 deleteLocalDatabase` | func | `storage.delete()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:970 dropLocalDatabaseConnection` | func | `end()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:977 reconnectLocalDatabase` | func | `storage.reconnect()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:995 catchUpToLive` | func | `catchUpToLive` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1003 inboxIdFromIdentity` | func | `inboxIdFromIdentity` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1009 signWithInstallationKey` | func | `signWithInstallationKey` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1013 verifySignature` | func | `verifySignature` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1024 verifySignatureWithInstallationId` | func | `verifySignatureWithInstallationID` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1038 inboxState` | func | `inboxState` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1046 inboxStatesForInboxIds` | func | `inboxStatesForInboxIDs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1055 syncAllDeviceSyncGroups` | func | `syncAllDeviceSyncGroups` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1061 createArchive` | func | `archives.exportToFile()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1069 importArchive` | func | `archives.importFromFile()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1073 archiveMetadata` | func | `archives.metadataFromFile()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1091 ffiApplySignatureRequest` | func | `unsafeApplySignatureRequest()` | alias | 11.4 Swift; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1108 ffiRevokeInstallations` | func | `unsafeRevokeInstallationsSignatureRequest()` | alias | 11.4 Swift; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1126 ffiRevokeAllOtherInstallations` | func | `unsafeRevokeAllOtherInstallationsSignatureRequest()` | alias | 11.4 Swift; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1143 ffiRevokeIdentity` | func | `unsafeRemoveAccountSignatureRequest()` | alias | 11.4 Swift; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1161 ffiAddIdentity` | func | `unsafeAddAccountSignatureRequest()` | alias | 11.4 Swift; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1196 ffiSignatureRequest` | func | `unsafeCreateInboxSignatureRequest()` | alias | 11.4 Swift; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1212 ffiRegisterIdentity` | func | `register()` | alias | 11.4 Swift; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1233 Client.serverConfiguration` | func | `Client.serverConfiguration` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1245 Client.refreshServerConfiguration` | func | `Client.refreshServerConfiguration` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1263 Client.fetchServerConfiguration` | func | `Client.fetchServerConfiguration` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1280 Client.LogLevel` | enum | `Client.LogLevel` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1311 Client.activatePersistentLibXMTPLogWriter` | func | `Client.activatePersistentLibXMTPLogWriter` | platform helper | 2 | Native lifecycle or log writer helper (2, 19.26/35). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1398 Client.deactivatePersistentLibXMTPLogWriter` | func | `Client.deactivatePersistentLibXMTPLogWriter` | platform helper | 2 | Native lifecycle or log writer helper (2, 19.26/35). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1412 Client.setLibXMTPNativeLogLevel` | func | `Client.setLibXMTPNativeLogLevel` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1426 Client.getXMTPLogFilePaths` | func | `Client.getXMTPLogFilePaths` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Client.swift:1464 Client.clearXMTPLogs` | func | `Client.clearXMTPLogs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:9 ContentTypeAttachment` | let | `ContentTypeAttachment` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:16 AttachmentCodecError` | enum | `AttachmentCodecError` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:17 AttachmentCodecError.invalidParameters` | case | `AttachmentCodecError.invalidParameters` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:17 AttachmentCodecError.unknownDecodingError` | case | `AttachmentCodecError.unknownDecodingError` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:20 Attachment` | struct | `Attachment` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:21 filename` | var | `filename` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:22 mimeType` | var | `mimeType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:23 data` | var | `data` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:25 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:32 AttachmentCodec` | struct | `AttachmentCodec` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:33 T` | typealias | `T` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:35 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:37 contentType` | var | `contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:39 encode` | func | `encode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:52 decode` | func | `decode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:62 fallback` | func | `fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/AttachmentCodec.swift:66 shouldPush` | func | `shouldPush` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentCodec.swift:14 EncodedContent` | typealias | `EncodedContent` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentCodec.swift:17 decoded` | func | `decoded` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentCodec.swift:80 ContentCodec` | protocol | `ContentCodec` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentCodec.swift:83 ContentCodec.contentType` | var | `ContentCodec.contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentCodec.swift:84 ContentCodec.encode` | func | `ContentCodec.encode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentCodec.swift:85 ContentCodec.decode` | func | `ContentCodec.decode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentCodec.swift:86 ContentCodec.fallback` | func | `ContentCodec.fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentCodec.swift:87 ContentCodec.shouldPush` | func | `ContentCodec.shouldPush` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentCodec.swift:91 ContentCodec.==` | func | `ContentCodec.==` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentCodec.swift:95 ContentCodec.id` | var | `ContentCodec.id` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentCodec.swift:99 ContentCodec.hash` | func | `ContentCodec.hash` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentCodec.swift:103 ContentCodec.description` | var | `ContentCodec.description` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentTypeID.swift:8 ContentTypeID` | typealias | `ContentTypeID` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentTypeID.swift:9 StandardContentType` | typealias | `StandardContentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentTypeID.swift:12 ContentTypeID.init` | init | `ContentTypeID.init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentTypeID.swift:22 ContentTypeID.id` | var | `ContentTypeID.id` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ContentTypeID.swift:26 ContentTypeID.description` | var | `ContentTypeID.description` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeleteMessageCodec.swift:3 ContentTypeDeleteMessageRequest` | let | `ContentTypeDeleteMessageRequest` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeleteMessageCodec.swift:13 DeleteMessageRequest` | struct | `DeleteMessageRequest` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeleteMessageCodec.swift:15 messageId` | var | `messageID` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeleteMessageCodec.swift:17 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeleteMessageCodec.swift:22 DeleteMessageCodec` | struct | `DeleteMessageCodec` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeleteMessageCodec.swift:23 T` | typealias | `T` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeleteMessageCodec.swift:25 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeleteMessageCodec.swift:27 contentType` | var | `contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeleteMessageCodec.swift:29 encode` | func | `encode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeleteMessageCodec.swift:36 decode` | func | `decode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeleteMessageCodec.swift:43 fallback` | func | `fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeleteMessageCodec.swift:47 shouldPush` | func | `shouldPush` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeletedMessage.swift:4 ContentTypeDeletedMessage` | let | `ContentTypeDeletedMessage` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeletedMessage.swift:13 DeletedMessage` | struct | `DeletedMessage` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeletedMessage.swift:14 deletedBy` | let | `deletedBy` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeletedMessage.swift:16 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeletedMessage.swift:22 DeletedBy` | enum | `DeletedBy` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeletedMessage.swift:24 DeletedBy.sender` | case | `DeletedBy.sender` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/DeletedMessage.swift:26 DeletedBy.admin` | case | `DeletedBy.admin` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/EncryptedEncodedContent.swift:10 EncryptedEncodedContent` | struct | `EncryptedEncodedContent` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/EncryptedEncodedContent.swift:11 secret` | var | `secret` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/EncryptedEncodedContent.swift:12 digest` | var | `digest` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/EncryptedEncodedContent.swift:13 salt` | var | `salt` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/EncryptedEncodedContent.swift:14 nonce` | var | `nonce` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/EncryptedEncodedContent.swift:15 payload` | var | `payload` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/EncryptedEncodedContent.swift:16 filename` | var | `filename` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/EncryptedEncodedContent.swift:17 contentLength` | var | `contentLength` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/EncryptedEncodedContent.swift:19 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/GroupUpdatedCodec.swift:10 GroupUpdated` | typealias | `GroupUpdated` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/GroupUpdatedCodec.swift:12 ContentTypeGroupUpdated` | let | `ContentTypeGroupUpdated` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/GroupUpdatedCodec.swift:19 GroupUpdatedCodec` | struct | `GroupUpdatedCodec` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/GroupUpdatedCodec.swift:20 T` | typealias | `T` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/GroupUpdatedCodec.swift:22 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/GroupUpdatedCodec.swift:24 contentType` | var | `contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/GroupUpdatedCodec.swift:26 encode` | func | `encode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/GroupUpdatedCodec.swift:35 decode` | func | `decode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/GroupUpdatedCodec.swift:39 fallback` | func | `fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/GroupUpdatedCodec.swift:43 shouldPush` | func | `shouldPush` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/LeaveRequestCodec.swift:3 ContentTypeLeaveRequest` | let | `ContentTypeLeaveRequest` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/LeaveRequestCodec.swift:16 LeaveRequest` | struct | `LeaveRequest` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/LeaveRequestCodec.swift:21 authenticatedNote` | var | `authenticatedNote` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/LeaveRequestCodec.swift:23 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/LeaveRequestCodec.swift:28 LeaveRequestCodec` | struct | `LeaveRequestCodec` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/LeaveRequestCodec.swift:29 T` | typealias | `T` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/LeaveRequestCodec.swift:31 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/LeaveRequestCodec.swift:33 contentType` | var | `contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/LeaveRequestCodec.swift:35 encode` | func | `encode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/LeaveRequestCodec.swift:42 decode` | func | `decode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/LeaveRequestCodec.swift:49 fallback` | func | `fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/LeaveRequestCodec.swift:53 shouldPush` | func | `shouldPush` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:5 ContentTypeMultiRemoteAttachment` | let | `ContentTypeMultiRemoteAttachment` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:12 MultiRemoteAttachmentError` | enum | `MultiRemoteAttachmentError` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:13 MultiRemoteAttachmentError.invalidDigest` | case | `MultiRemoteAttachmentError.invalidDigest` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:13 MultiRemoteAttachmentError.invalidParameters` | case | `MultiRemoteAttachmentError.invalidParameters` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:13 MultiRemoteAttachmentError.invalidScheme` | case | `MultiRemoteAttachmentError.invalidScheme` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:13 MultiRemoteAttachmentError.invalidURL` | case | `MultiRemoteAttachmentError.invalidURL` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:13 MultiRemoteAttachmentError.v1NotSupported` | case | `MultiRemoteAttachmentError.v1NotSupported` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:16 description` | var | `description` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:34 MultiRemoteAttachment` | struct | `MultiRemoteAttachment` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:35 Scheme` | enum | `Scheme` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:36 Scheme.https` | case | `Scheme.https` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:39 remoteAttachments` | let | `remoteAttachments` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:41 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:45 RemoteAttachmentInfo` | struct | `RemoteAttachmentInfo` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:46 url` | let | `url` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:47 filename` | let | `filename` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:48 contentLength` | let | `contentLength` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:49 contentDigest` | let | `contentDigest` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:50 nonce` | let | `nonce` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:51 scheme` | let | `scheme` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:52 salt` | let | `salt` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:53 secret` | let | `secret` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:55 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:75 from` | func | `from` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:94 MultiRemoteAttachmentCodec` | struct | `MultiRemoteAttachmentCodec` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:95 T` | typealias | `T` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:97 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:99 contentType` | var | `contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:101 encode` | func | `encode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:118 decode` | func | `decode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:135 fallback` | func | `fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/MultiRemoteAttachmentCodec.swift:139 shouldPush` | func | `shouldPush` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:10 ContentTypeReaction` | let | `ContentTypeReaction` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:17 Reaction` | struct | `Reaction` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:18 reference` | var | `reference` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:19 referenceInboxId` | var | `referenceInboxID` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:20 action` | var | `action` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:21 content` | var | `content` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:22 schema` | var | `schema` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:24 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:39 ReactionAction` | enum | `ReactionAction` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:40 ReactionAction.added` | case | `ReactionAction.added` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:40 ReactionAction.removed` | case | `ReactionAction.removed` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:40 ReactionAction.unknown` | case | `ReactionAction.unknown` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:42 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:54 ReactionSchema` | enum | `ReactionSchema` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:55 ReactionSchema.custom` | case | `ReactionSchema.custom` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:55 ReactionSchema.shortcode` | case | `ReactionSchema.shortcode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:55 ReactionSchema.unicode` | case | `ReactionSchema.unicode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:55 ReactionSchema.unknown` | case | `ReactionSchema.unknown` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:57 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:71 ReactionCodec` | struct | `ReactionCodec` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:72 T` | typealias | `T` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:73 contentType` | var | `contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:75 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:77 encode` | func | `encode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:86 decode` | func | `decode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:104 fallback` | func | `fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionCodec.swift:115 shouldPush` | func | `shouldPush` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionV2Codec.swift:10 ContentTypeReactionV2` | let | `ContentTypeReactionV2` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionV2Codec.swift:17 ReactionV2Codec` | struct | `ReactionV2Codec` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionV2Codec.swift:18 T` | typealias | `T` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionV2Codec.swift:19 contentType` | var | `contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionV2Codec.swift:21 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionV2Codec.swift:23 encode` | func | `encode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionV2Codec.swift:35 decode` | func | `decode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionV2Codec.swift:47 fallback` | func | `fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReactionV2Codec.swift:58 shouldPush` | func | `shouldPush` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReadReceiptCodec.swift:10 ContentTypeReadReceipt` | let | `ContentTypeReadReceipt` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReadReceiptCodec.swift:17 ReadReceipt` | struct | `ReadReceipt` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReadReceiptCodec.swift:18 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReadReceiptCodec.swift:21 ReadReceiptCodec` | struct | `ReadReceiptCodec` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReadReceiptCodec.swift:22 T` | typealias | `T` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReadReceiptCodec.swift:24 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReadReceiptCodec.swift:26 contentType` | var | `contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReadReceiptCodec.swift:28 encode` | func | `encode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReadReceiptCodec.swift:37 decode` | func | `decode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReadReceiptCodec.swift:41 fallback` | func | `fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReadReceiptCodec.swift:45 shouldPush` | func | `shouldPush` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:5 ContentTypeRemoteAttachment` | let | `ContentTypeRemoteAttachment` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:12 RemoteAttachmentError` | enum | `RemoteAttachmentError` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:13 RemoteAttachmentError.invalidDigest` | case | `RemoteAttachmentError.invalidDigest` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:13 RemoteAttachmentError.invalidParameters` | case | `RemoteAttachmentError.invalidParameters` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:13 RemoteAttachmentError.invalidScheme` | case | `RemoteAttachmentError.invalidScheme` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:13 RemoteAttachmentError.invalidURL` | case | `RemoteAttachmentError.invalidURL` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:13 RemoteAttachmentError.v1NotSupported` | case | `RemoteAttachmentError.v1NotSupported` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:16 description` | var | `description` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:48 RemoteAttachment` | struct | `RemoteAttachment` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:49 Scheme` | enum | `Scheme` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:50 Scheme.https` | case | `Scheme.https` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:53 url` | var | `url` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:54 contentDigest` | var | `contentDigest` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:55 secret` | var | `secret` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:56 salt` | var | `salt` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:57 nonce` | var | `nonce` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:58 scheme` | var | `scheme` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:62 contentLength` | var | `contentLength` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:63 filename` | var | `filename` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:65 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:89 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:109 encodeEncrypted` | func | `encodeEncrypted` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:126 encodeEncryptedBytes` | func | `encodeEncryptedBytes` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:141 decryptEncoded` | func | `decryptEncoded` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:161 content` | func | `RemoteAttachmentDownload` | platform helper | 2 | Native HTTPS download stays under sdks/ (2, 11.4). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:182 RemoteAttachmentCodec` | struct | `RemoteAttachmentCodec` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:183 T` | typealias | `T` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:185 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:187 contentType` | var | `contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:189 encode` | func | `encode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:210 decode` | func | `decode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:251 fallback` | func | `fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/RemoteAttachmentCodec.swift:263 shouldPush` | func | `shouldPush` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:10 ContentTypeReply` | let | `ContentTypeReply` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:12 Reply` | struct | `Reply` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:13 reference` | var | `reference` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:14 content` | var | `content` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:15 contentType` | var | `contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:16 inReplyTo` | var | `inReplyTo` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:18 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:25 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:33 ReplyCodec` | struct | `ReplyCodec` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:34 contentType` | var | `contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:36 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:38 encode` | func | `encode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:51 decode` | func | `decode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:75 fallback` | func | `fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/ReplyCodec.swift:79 shouldPush` | func | `shouldPush` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TextCodec.swift:10 ContentTypeText` | let | `ContentTypeText` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TextCodec.swift:16 TextCodec` | struct | `TextCodec` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TextCodec.swift:17 T` | typealias | `T` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TextCodec.swift:19 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TextCodec.swift:21 contentType` | var | `contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TextCodec.swift:23 encode` | func | `encode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TextCodec.swift:33 decode` | func | `decode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TextCodec.swift:45 fallback` | func | `fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TextCodec.swift:49 shouldPush` | func | `shouldPush` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:10 ContentTypeTransactionReference` | let | `ContentTypeTransactionReference` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:17 TransactionReference` | struct | `TransactionReference` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:18 Metadata` | struct | `Metadata` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:19 transactionType` | let | `transactionType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:20 currency` | let | `currency` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:21 amount` | let | `amount` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:22 decimals` | let | `decimals` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:23 fromAddress` | let | `fromAddress` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:24 toAddress` | let | `toAddress` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:26 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:43 namespace` | let | `namespace` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:44 networkId` | let | `networkID` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:45 reference` | let | `reference` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:46 metadata` | let | `kind / creatorInboxID` | alias | 11.4 Swift; 19.2 | Deprecated name for one major release (19.2; 11.4). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:48 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:61 TransactionReferenceCodec` | struct | `TransactionReferenceCodec` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:62 T` | typealias | `T` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:64 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:66 contentType` | var | `contentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:68 encode` | func | `encode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:91 decode` | func | `decode` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:115 fallback` | func | `fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Codecs/TransactionReferenceCodec.swift:119 shouldPush` | func | `shouldPush` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:3 Conversation` | enum | `Conversation` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:4 Conversation.group` | case | `Conversation.group` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:5 Conversation.dm` | case | `Conversation.dm` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:7 ==` | func | `==` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:11 hash` | func | `hash` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:15 XMTPConversationType` | enum | `XMTPConversationType` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:16 XMTPConversationType.dm` | case | `XMTPConversationType.dm` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:16 XMTPConversationType.group` | case | `XMTPConversationType.group` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:19 id` | var | `id` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:28 disappearingMessageSettings` | var | `state().disappearingSettings` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:37 isDisappearingMessagesEnabled` | func | `state().isDisappearingEnabled` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:46 lastMessage` | func | `lastMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:55 commitLogForkStatus` | func | `state().commitLogForkStatus` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:64 isCreator` | func | `isCreator` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:73 members` | func | `members` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:82 consentState` | func | `state().consentState` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:91 updateConsentState` | func | `updateConsentState` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:100 updateDisappearingMessageSettings` | func | `updateDisappearingMessageSettings` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:115 clearDisappearingMessageSettings` | func | `clearDisappearingMessageSettings` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:124 sync` | func | `sync` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:133 processMessage` | func | `processMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:142 prepareMessage` | func | `prepareMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:165 prepareMessage` | func | `prepareMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:180 publishMessages` | func | `publishMessages` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:189 publishMessage` | func | `publishMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:198 type` | var | `type` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:207 createdAt` | var | `createdAt` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:216 createdAtNs` | var | `createdAt.ns` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:225 lastActivityAtNs` | var | `lastActivityAtNs(contentTypes?)` | generated | 11.4 Swift | Optional content-type filter; outside state() (plan Decisions, 19.46). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:234 send` | func | `send` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:245 send` | func | `send` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:260 send` | func | `send` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:271 topic` | var | `topic` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:281 streamMessages` | func | `stream()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:292 messageReader` | func | `messageReader` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:299 messageHistorySnapshot` | func | `messageHistorySnapshot` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:306 beginningDeliveryCursor` | func | `beginningDeliveryCursor` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:324 messages` | func | `messages` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:361 pausedForVersion` | func | `state().pausedForVersion` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:370 clientInboxId` | var | `clientInboxID` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:394 enrichedMessages` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:430 countMessages` | func | `countMessages` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:459 getHmacKeys` | func | `hmacKeys()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:469 setNotifications` | func | `setNotifications` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:479 notificationsEnabled` | func | `state().notificationsEnabled` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:488 getDebugInformation` | func | `debugInfo()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:497 isActive` | func | `state().isActive` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:508 getLastReadTimes` | func | `lastReadTimes()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversation.swift:521 deleteMessage` | func | `deleteMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:3 ConversationError` | enum | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:4 ConversationError.memberCannotBeSelf` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:5 ConversationError.memberNotRegistered` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:6 ConversationError.groupsRequireMessagePassed` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:6 ConversationError.notSupportedByGroups` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:6 ConversationError.streamingFailure` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:8 description` | var | `description` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:23 errorDescription` | var | `errorDescription` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:28 GroupSyncSummary` | struct | `GroupSyncSummary` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:29 numEligible` | var | `numEligible` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:30 numSynced` | var | `numSynced` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:32 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:43 ConversationFilterType` | enum | `ConversationKind` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:44 ConversationFilterType.all` | case | `ConversationFilterType.all` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:44 ConversationFilterType.dms` | case | `ConversationFilterType.dms` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:44 ConversationFilterType.groups` | case | `ConversationFilterType.groups` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:47 ConversationsOrderBy` | enum | `ConversationOrder` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:48 ConversationsOrderBy.createdAt` | case | `ConversationsOrderBy.createdAt` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:48 ConversationsOrderBy.lastActivity` | case | `ConversationsOrderBy.lastActivity` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:121 Conversations` | class | `Conversations` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:152 findGroup` | func | `getByID()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:165 findConversation` | func | `getByID()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:180 findConversationByTopic` | func | `getByID()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:207 findDmByInboxId` | func | `getDmByInboxID()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:221 findDmByIdentity` | func | `getDmByIdentity()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:234 findMessage` | func | `getMessageByID()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:246 findEnrichedMessage` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:257 deleteMessageLocally` | func | `deleteMessageLocally` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:261 sync` | func | `sync` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:265 syncAllConversations` | func | `syncAll()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:274 listGroups` | func | `listGroups` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:306 listDms` | func | `listDms` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:339 list` | func | `list` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:376 stream` | func | `stream` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:447 newConversationWithIdentity` | func | `newConversationWithIdentity` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:458 findOrCreateDmWithIdentity` | func | `findOrCreateDmWithIdentity` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:486 newConversation` | func | `createDm()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:497 findOrCreateDm` | func | `findOrCreateDm` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:520 newGroupWithIdentities` | func | `newGroupWithIdentities` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:544 newGroupCustomPermissionsWithIdentities` | func | `newGroupCustomPermissionsWithIdentities` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:593 newGroup` | func | `createGroup()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:617 newGroupCustomPermissions` | func | `newGroupCustomPermissions` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:667 newGroupOptimistic` | func | `newGroupOptimistic` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:695 streamAllMessages` | func | `streamAllMessages` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:724 messageReader` | func | `messageReader` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:739 messageHistorySnapshot` | func | `messageHistorySnapshot` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:753 beginningDeliveryCursor` | func | `beginningDeliveryCursor` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:759 streamMessageDeletions` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:798 fromWelcome` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Conversations.swift:812 getHmacKeys` | func | `hmacKeys()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Crypto.swift:4 CipherText` | typealias | — | approved removal | 11.4 Swift | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:3 Dm` | struct | `Dm` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:9 clientInboxId` | var | `clientInboxID` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:12 ConversationError` | enum | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:13 ConversationError.missingPeerInboxId` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:15 description` | var | `state().description` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:22 errorDescription` | var | `errorDescription` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:27 id` | var | `id` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:31 topic` | var | `topic` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:35 disappearingMessageSettings` | var | `state().disappearingSettings` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:43 isDisappearingMessagesEnabled` | func | `state().isDisappearingEnabled` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:51 sync` | func | `sync` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:55 ==` | func | `==` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:59 hash` | func | `hash` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:63 isCreator` | func | `isCreator` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:67 isActive` | func | `state().isActive` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:71 creatorInboxId` | func | `creatorInboxID` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:75 addedByInboxId` | func | `addedByInboxID` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:79 members` | var | `members` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:88 peerInboxId` | var | `peerInboxID` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:97 createdAt` | var | `createdAt` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:101 createdAtNs` | var | `createdAt.ns` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:105 lastActivityAtNs` | var | `lastActivityAtNs(contentTypes?)` | generated | 11.4 Swift | Optional content-type filter; outside state() (plan Decisions, 19.46). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:109 updateConsentState` | func | `updateConsentState` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:113 consentState` | func | `state().consentState` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:117 updateDisappearingMessageSettings` | func | `updateDisappearingMessageSettings` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:134 clearDisappearingMessageSettings` | func | `clearDisappearingMessageSettings` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:139 pausedForVersion` | func | `state().pausedForVersion` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:143 processMessage` | func | `processMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:154 send` | func | `send` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:163 send` | func | `send` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:174 encodeContent` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:227 prepareMessage` | func | `prepareMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:252 prepareMessage` | func | `prepareMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:265 publishMessages` | func | `publishMessages` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:269 publishMessage` | func | `publishMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:273 endStream` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:278 streamMessages` | func | `stream()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:286 messageReader` | func | `messageReader` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:290 messageHistorySnapshot` | func | `messageHistorySnapshot` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:294 beginningDeliveryCursor` | func | `beginningDeliveryCursor` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:298 lastMessage` | func | `lastMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:306 commitLogForkStatus` | func | `state().commitLogForkStatus` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:325 messages` | func | `messages` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:397 countMessages` | func | `countMessages` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:436 enrichedMessages` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:507 getHmacKeys` | func | `hmacKeys()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:533 setNotifications` | func | `setNotifications` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:538 notificationsEnabled` | func | `state().notificationsEnabled` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:542 getDebugInformation` | func | `debugInfo()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:548 getLastReadTimes` | func | `lastReadTimes()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Dm.swift:556 deleteMessage` | func | `deleteMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/EncodedContentCompression.swift:4 EncodedContentCompression` | enum | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/EncodedContentCompression.swift:5 EncodedContentCompression.deflate` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/EncodedContentCompression.swift:6 EncodedContentCompression.gzip` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Extensions/String.swift:5 String.hexToData` | var | `String.hexToData` | static runtime | 2; open | Proposed host helper; the design does not name this extension export. |
| `sdks/ios/Sources/XMTPiOS/Group.swift:3 GroupMembershipState` | enum | `GroupMembershipState` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:4 GroupMembershipState.allowed` | case | `GroupMembershipState.allowed` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:4 GroupMembershipState.pending` | case | `GroupMembershipState.pending` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:4 GroupMembershipState.pendingRemove` | case | `GroupMembershipState.pendingRemove` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:4 GroupMembershipState.rejected` | case | `GroupMembershipState.rejected` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:4 GroupMembershipState.restored` | case | `GroupMembershipState.restored` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:7 Group` | struct | `Group` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:13 clientInboxId` | var | `clientInboxID` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:16 id` | var | `id` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:20 topic` | var | `topic` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:24 disappearingMessageSettings` | var | `state().disappearingSettings` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:32 isDisappearingMessagesEnabled` | func | `state().isDisappearingEnabled` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:44 sync` | func | `sync` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:48 ==` | func | `==` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:52 hash` | func | `hash` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:56 isActive` | func | `state().isActive` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:60 isCreator` | func | `isCreator` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:64 isAdmin` | func | `isAdmin` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:68 isSuperAdmin` | func | `isSuperAdmin` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:72 addAdmin` | func | `addAdmin` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:76 removeAdmin` | func | `removeAdmin` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:80 addSuperAdmin` | func | `addSuperAdmin` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:84 removeSuperAdmin` | func | `removeSuperAdmin` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:88 listAdmins` | func | `listAdmins` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:92 listSuperAdmins` | func | `listSuperAdmins` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:96 permissionPolicySet` | func | `state().permissions.policySet` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:102 creatorInboxId` | func | `creatorInboxID` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:106 addedByInboxId` | func | `addedByInboxID` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:110 members` | var | `members` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:118 membershipState` | var | `state().membershipState` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:124 peerInboxIds` | var | `peerInboxIDs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:134 createdAt` | var | `createdAt` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:138 createdAtNs` | var | `createdAt.ns` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:142 lastActivityAtNs` | var | `lastActivityAtNs(contentTypes?)` | generated | 11.4 Swift | Optional content-type filter; outside state() (plan Decisions, 19.46). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:146 addMembers` | func | `addMembers` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:154 removeMembers` | func | `removeMembers` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:159 addMembersByIdentity` | func | `addMembersByIdentity` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:168 removeMembersByIdentity` | func | `removeMembersByIdentity` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:176 name` | func | `state().name` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:180 imageUrl` | func | `state().imageUrl` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:184 description` | func | `state().description` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:188 appData` | func | `state().appData` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:192 updateName` | func | `updateName` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:196 updateImageUrl` | func | `updateImageUrl` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:202 updateDescription` | func | `updateDescription` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:217 updateAppData` | func | `updateAppData` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:243 proposalsEnabled` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Group.swift:258 membershipCapabilities` | func | `membershipCapabilities` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:262 updateAddMemberPermission` | func | `updateAddMemberPermission` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:273 updateRemoveMemberPermission` | func | `updateRemoveMemberPermission` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:284 updateAddAdminPermission` | func | `updateAddAdminPermission` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:295 updateRemoveAdminPermission` | func | `updateRemoveAdminPermission` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:306 updateNamePermission` | func | `updateNamePermission` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:318 updateDescriptionPermission` | func | `updateDescriptionPermission` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:330 updateImageUrlPermission` | func | `updateImageUrlPermission` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:342 updateDisappearingMessageSettings` | func | `updateDisappearingMessageSettings` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:358 clearDisappearingMessageSettings` | func | `clearDisappearingMessageSettings` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:363 pausedForVersion` | func | `state().pausedForVersion` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:367 updateConsentState` | func | `updateConsentState` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:371 consentState` | func | `state().consentState` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:375 processMessage` | func | `processMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:387 send` | func | `send` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:396 send` | func | `send` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:411 encodeContent` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Group.swift:464 prepareMessage` | func | `prepareMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:489 prepareMessage` | func | `prepareMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:502 publishMessages` | func | `publishMessages` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:506 publishMessage` | func | `publishMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:510 endStream` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Group.swift:515 streamMessages` | func | `stream()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:523 messageReader` | func | `messageReader` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:527 messageHistorySnapshot` | func | `messageHistorySnapshot` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:531 beginningDeliveryCursor` | func | `beginningDeliveryCursor` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:535 lastMessage` | func | `lastMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:543 commitLogForkStatus` | func | `state().commitLogForkStatus` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:562 messages` | func | `messages` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:648 enrichedMessages` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Group.swift:719 countMessages` | func | `countMessages` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:743 getHmacKeys` | func | `hmacKeys()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:769 setNotifications` | func | `setNotifications` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:774 notificationsEnabled` | func | `state().notificationsEnabled` | generated | 11.4 Swift | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:778 getDebugInformation` | func | `debugInfo()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:784 getLastReadTimes` | func | `lastReadTimes()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:788 leaveGroup` | func | `requestRemoval()` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Group.swift:796 deleteMessage` | func | `deleteMessage` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:10 ArchiveOptions` | struct | `ArchiveOptions` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:11 startNs` | var | `startNs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:12 endNs` | var | `endNs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:13 archiveElements` | var | `archiveElements` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:14 excludeDisappearingMessages` | var | `excludeDisappearingMessages` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:16 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:28 toFfi` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:38 ArchiveElement` | enum | `ArchiveElement` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:39 ArchiveElement.messages` | case | `ArchiveElement.messages` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:40 ArchiveElement.consent` | case | `ArchiveElement.consent` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:42 toFfi` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:51 fromFfi` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:63 ArchiveMetadata` | struct | `ArchiveMetadata` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:66 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:70 archiveVersion` | var | `archiveVersion` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:74 elements` | var | `elements` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:78 exportedAtNs` | var | `exportedAt.ns` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:82 startNs` | var | `startNs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ArchiveOptions.swift:86 endNs` | var | `endNs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ConversationDebugInfo.swift:1 CommitLogForkStatus` | enum | `CommitLogForkStatus` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ConversationDebugInfo.swift:2 CommitLogForkStatus.forked` | case | `CommitLogForkStatus.forked` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ConversationDebugInfo.swift:3 CommitLogForkStatus.notForked` | case | `CommitLogForkStatus.notForked` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ConversationDebugInfo.swift:4 CommitLogForkStatus.unknown` | case | `CommitLogForkStatus.unknown` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ConversationDebugInfo.swift:7 ConversationDebugInfo` | struct | `ConversationDebugInfo` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ConversationDebugInfo.swift:10 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ConversationDebugInfo.swift:14 epoch` | var | `epoch` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ConversationDebugInfo.swift:18 maybeForked` | var | `maybeForked` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ConversationDebugInfo.swift:22 forkDetails` | var | `forkDetails` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ConversationDebugInfo.swift:26 localCommitLog` | var | `localCommitLog` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ConversationDebugInfo.swift:30 remoteCommitLog` | var | `remoteCommitLog` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/ConversationDebugInfo.swift:34 commitLogForkStatus` | var | `commitLogForkStatus` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:7 MessageDeliveryStatus` | enum | `MessageDeliveryStatus` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:8 MessageDeliveryStatus.all` | case | `MessageDeliveryStatus.all` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:9 MessageDeliveryStatus.published` | case | `MessageDeliveryStatus.published` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:10 MessageDeliveryStatus.unpublished` | case | `MessageDeliveryStatus.unpublished` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:11 MessageDeliveryStatus.failed` | case | `MessageDeliveryStatus.failed` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:38 SortDirection` | enum | `SortDirection` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:39 SortDirection.ascending` | case | `SortDirection.ascending` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:40 SortDirection.descending` | case | `SortDirection.descending` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:61 MessageSortBy` | enum | `MessageSortBy` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:62 MessageSortBy.sentAt` | case | `MessageSortBy.sentAt` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:63 MessageSortBy.insertedAt` | case | `MessageSortBy.insertedAt` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:84 DecodedMessage` | struct | `Message` | alias | 11.4 Swift; 19.2 | Deprecated name for one major release (19.2; 11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:88 deliveryCursor` | let | `deliveryCursor` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:90 id` | var | `id` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:94 conversationId` | var | `conversationID` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:98 senderInboxId` | var | `senderInboxID` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:102 kind` | var | `kind` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:106 sentAt` | var | `sentAt` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:113 sentAtNs` | var | `sentAt.ns` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:117 insertedAt` | var | `insertedAt` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:124 insertedAtNs` | var | `insertedAt.ns` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:128 expiresAtNs` | var | `expiresAt.ns` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:132 expiresAt` | var | `expiresAt` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:136 deliveryStatus` | var | `deliveryStatus` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:147 topic` | var | `topic` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:151 content` | func | `content` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:160 fallback` | var | `fallback` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:166 body` | var | `body` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:176 encodedContent` | var | `encodedContent` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessage.swift:182 create` | func | `create` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:3 Intent` | typealias | `Intent` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:4 Actions` | typealias | `Actions` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:6 DecodedMessageV2` | struct | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:9 id` | var | `Message.id` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:13 conversationId` | var | `Message.conversationID` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:17 senderInboxId` | var | `Message.senderInboxID` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:21 sentAt` | var | `Message.sentAt` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:25 sentAtNs` | var | `Message.sentAt.ns` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:29 insertedAt` | var | `Message.insertedAt` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:33 insertedAtNs` | var | `Message.insertedAt.ns` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:37 expiresAtNs` | var | `Message.expiresAt.ns` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:41 expiresAt` | var | `Message.expiresAt` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:45 deliveryStatus` | var | `Message.deliveryStatus` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:56 topic` | var | `Message.topic` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:60 reactions` | var | `Message.reactions` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:66 content` | func | `Message.content` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:76 fallback` | var | `Message.fallback` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:94 body` | var | — | approved removal | 11.4 Swift | Merged into the Message value model (11.4, 19.5). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:104 contentTypeId` | var | `Message.contentTypeID` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:114 init` | init | `Message.init` | static runtime | 11.4 Swift | Old live getter moves to the Message host value (11.7). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DecodedMessageV2.swift:118 create` | func | — | approved removal | 11.4 Swift | Merged into the Message value model (11.4, 19.5). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DisappearingMessageSettings.swift:8 DisappearingMessageSettings` | struct | `DisappearingMessageSettings` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DisappearingMessageSettings.swift:9 disappearStartingAtNs` | let | `disappearStartingAt.ns` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DisappearingMessageSettings.swift:10 retentionDurationInNs` | let | `retentionDurationInNs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/DisappearingMessageSettings.swift:12 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:10 MlsExtensionType` | enum | `MlsExtensionType` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:11 MlsExtensionType.applicationId` | case | `MlsExtensionType.applicationID` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:12 MlsExtensionType.ratchetTree` | case | `MlsExtensionType.ratchetTree` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:13 MlsExtensionType.requiredCapabilities` | case | `MlsExtensionType.requiredCapabilities` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:14 MlsExtensionType.externalPub` | case | `MlsExtensionType.externalPub` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:15 MlsExtensionType.externalSenders` | case | `MlsExtensionType.externalSenders` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:16 MlsExtensionType.lastResort` | case | `MlsExtensionType.lastResort` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:17 MlsExtensionType.immutableMetadata` | case | `MlsExtensionType.immutableMetadata` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:18 MlsExtensionType.appDataDictionary` | case | `MlsExtensionType.appDataDictionary` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:19 MlsExtensionType.unknown` | case | `MlsExtensionType.unknown` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:20 MlsExtensionType.grease` | case | `MlsExtensionType.grease` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:39 InstallationCapabilities` | struct | `InstallationCapabilities` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:42 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:47 installationId` | var | `installationID` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:52 isOwn` | var | `isOwn` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:58 supportedExtensions` | var | `supportedExtensions` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:65 capabilitiesKnown` | var | `capabilitiesKnown` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:72 InboxCapabilities` | struct | `InboxCapabilities` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:75 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:79 inboxId` | var | `inboxID` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:83 installations` | var | `installations` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:102 GroupMembershipCapabilities` | struct | `GroupMembershipCapabilities` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:105 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:110 contextExtensions` | var | `contextExtensions` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipCapabilities.swift:116 members` | var | `members` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipResult.swift:10 GroupMembershipResult` | struct | `GroupMembershipResult` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipResult.swift:13 addedMembers` | var | `addedMembers` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipResult.swift:17 removedMembers` | var | `removedMembers` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/GroupMembershipResult.swift:21 failedInstallationIds` | var | `failedInstallationIDs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/InboxState.swift:10 SignatureKind` | typealias | `SignatureKind` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/InboxState.swift:12 InboxState` | struct | `InboxState` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/InboxState.swift:15 inboxId` | var | `inboxID` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/InboxState.swift:19 identities` | var | `identities` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/InboxState.swift:23 installations` | var | `installations` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/InboxState.swift:27 recoveryIdentity` | var | `recoveryIdentity` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/InboxState.swift:33 creationSignatureKind` | var | `creationSignatureKind` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/Installation.swift:10 Installation` | struct | `Installation` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/Installation.swift:13 id` | var | `id` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/Installation.swift:17 createdAt` | var | `createdAt` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/Member.swift:10 PermissionLevel` | enum | `PermissionLevel` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/Member.swift:11 PermissionLevel.Admin` | case | `PermissionLevel.Admin` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/Member.swift:11 PermissionLevel.Member` | case | `PermissionLevel.Member` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/Member.swift:11 PermissionLevel.SuperAdmin` | case | `PermissionLevel.SuperAdmin` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/Member.swift:14 Member` | struct | `Member` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/Member.swift:17 inboxId` | var | `inboxID` | alias | 11.4 Swift; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/Member.swift:21 identities` | var | `identities` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/Member.swift:25 permissionLevel` | var | `permissionLevel` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/Member.swift:36 consentState` | var | `consentState` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:3 PermissionOption` | enum | `PermissionOption` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:4 PermissionOption.allow` | case | `PermissionOption.allow` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:5 PermissionOption.deny` | case | `PermissionOption.deny` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:6 PermissionOption.admin` | case | `PermissionOption.admin` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:7 PermissionOption.superAdmin` | case | `PermissionOption.superAdmin` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:8 PermissionOption.unknown` | case | `PermissionOption.unknown` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:45 GroupPermissionPreconfiguration` | enum | `GroupPermissionPreconfiguration` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:46 GroupPermissionPreconfiguration.allMembers` | case | `GroupPermissionPreconfiguration.allMembers` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:47 GroupPermissionPreconfiguration.adminOnly` | case | `GroupPermissionPreconfiguration.adminOnly` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:61 PermissionPolicySet` | class | `PermissionPolicySet` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:62 addMemberPolicy` | var | `addMemberPolicy` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:63 removeMemberPolicy` | var | `removeMemberPolicy` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:64 addAdminPolicy` | var | `addAdminPolicy` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:65 removeAdminPolicy` | var | `removeAdminPolicy` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:66 updateGroupNamePolicy` | var | `updateGroupNamePolicy` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:67 updateGroupDescriptionPolicy` | var | `updateGroupDescriptionPolicy` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:68 updateGroupImagePolicy` | var | `updateGroupImagePolicy` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:69 updateMessageDisappearingPolicy` | var | `updateMessageDisappearingPolicy` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:70 updateAppDataPolicy` | var | `updateAppDataPolicy` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PermissionPolicySet.swift:72 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PublicIdentity.swift:10 IdentityKind` | enum | `IdentityKind` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PublicIdentity.swift:11 IdentityKind.ethereum` | case | `IdentityKind.ethereum` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PublicIdentity.swift:12 IdentityKind.passkey` | case | `IdentityKind.passkey` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PublicIdentity.swift:15 PublicIdentity` | struct | `PublicIdentity` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PublicIdentity.swift:18 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PublicIdentity.swift:29 kind` | var | `kind` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PublicIdentity.swift:33 identifier` | var | `identifier` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PublicIdentity.swift:39 IdentityKind.toFfiPublicIdentifierKind` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/PublicIdentity.swift:50 FfiIdentifierKind.toIdentityKind` | func | `FfiIdentifierKind.toIdentityKind` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/SignatureRequest.swift:10 SignatureRequest` | struct | `SignatureRequest` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/SignatureRequest.swift:11 ffiSignatureRequest` | let | `unsafeCreateInboxSignatureRequest()` | alias | 11.4 Swift; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/SignatureRequest.swift:13 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/SignatureRequest.swift:17 addScwSignature` | func | `addScwSignature` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/SignatureRequest.swift:31 addEcdsaSignature` | func | `addEcdsaSignature` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/SignatureRequest.swift:37 signatureText` | func | `signatureText` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:10 XMTPDebugInformation` | class | `XMTPDebugInformation` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:13 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:17 apiStatistics` | var | `apiStatistics` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:21 identityStatistics` | var | `identityStatistics` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:25 aggregateStatistics` | var | `aggregateStatistics` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:29 clearAllStatistics` | func | `clearAllStatistics` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:34 uploadDebugInformation` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:40 ApiStats` | class | `ApiStats` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:43 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:47 publish` | var | `publish` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:51 query` | var | `query` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:55 queryNewest` | var | `queryNewest` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:59 subscribe` | var | `subscribe` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:63 subscribeStatic` | var | `subscribeStatic` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:68 IdentityStats` | class | `IdentityStats` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:71 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:75 getInboxIds` | var | `getInboxIDs` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/XMTPDebugInformation.swift:79 verifySmartContractWalletSignatures` | var | `verifySmartContractWalletSignatures` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Libxmtp/xmtpv3.swift:0 all public declarations` (2455 declarations) | generated family | `facade-generated bindings` | generated | 2; 11.4 Swift | Old UniFFI output is replaced from the facade (2, 19.45). |
| `sdks/ios/Sources/XMTPiOS/MessageReader.swift:3 DeliveryCursor` | typealias | `DeliveryCursor` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/MessageReader.swift:4 MessageCatchUpSnapshot` | typealias | `MessageCatchUpSnapshot` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/MessageReader.swift:7 MessageHistorySnapshot` | struct | `MessageHistorySnapshot` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/MessageReader.swift:8 messages` | let | `messages` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/MessageReader.swift:9 cursor` | let | `cursor` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/MessageReader.swift:30 MessageReader` | class | `MessageReader` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/MessageReader.swift:61 next` | func | `next` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/MessageReader.swift:65 messages` | func | `messages` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/MessageReader.swift:72 close` | func | `end()` | alias | 11.4 Swift; 19.2 | Async client and reader shutdown uses end() (plan Decisions, Section 20.8). |
| `sdks/ios/Sources/XMTPiOS/MessageReader.swift:78 updateScope` | func | `updateScope` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/MessageReader.swift:82 updateFilter` | func | `updateFilter` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/MessageReader.swift:86 catchUpSnapshot` | func | `catchUpSnapshot` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/MessageReader.swift:90 catchUpChanged` | func | `catchUpChanged` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/MessageVisibilityOptions.swift:11 MessageVisibilityOptions` | struct | `MessageVisibilityOptions` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/MessageVisibilityOptions.swift:13 shouldPush` | var | `shouldPush` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/MessageVisibilityOptions.swift:17 idempotencyKey` | var | `idempotencyKey` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/MessageVisibilityOptions.swift:23 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/MessageVisibilityOptions.swift:30 toFfi` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Messages/PrivateKey.swift:5 PrivateKey` | typealias | — | approved removal | 11.4 Swift | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/ios/Sources/XMTPiOS/Messages/PrivateKey.swift:24 identity` | var | — | approved removal | 11.4 Swift | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/ios/Sources/XMTPiOS/Messages/PrivateKey.swift:28 sign` | func | — | approved removal | 11.4 Swift | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/ios/Sources/XMTPiOS/Messages/PrivateKey.swift:47 PrivateKey.generate` | func | — | approved removal | 11.4 Swift | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/ios/Sources/XMTPiOS/Messages/PrivateKey.swift:53 PrivateKey.init` | init | — | approved removal | 11.4 Swift | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:4 NotificationChannel` | enum | `NotificationChannel` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:5 NotificationChannel.apns` | case | `NotificationChannel.apns` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:6 NotificationChannel.fcm` | case | `NotificationChannel.fcm` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:7 NotificationChannel.http` | case | `NotificationChannel.http` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:19 NotificationConfig` | struct | `NotificationConfig` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:20 channel` | var | `channel` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:21 consentStates` | var | `consentStates` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:22 includeWelcomes` | var | `includeWelcomes` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:23 includeSyncGroups` | var | `includeSyncGroups` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:24 includeCommits` | var | `includeCommits` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:26 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:52 NotificationOverride` | enum | `NotificationOverride` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:53 NotificationOverride.disabled` | case | `NotificationOverride.disabled` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:53 NotificationOverride.enabled` | case | `NotificationOverride.enabled` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:65 NotificationError` | struct | `NotificationError` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:66 code` | let | `code` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:93 NotificationState` | enum | `NotificationState` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:94 NotificationState.disabled` | case | `NotificationState.disabled` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:94 NotificationState.enabled` | case | `NotificationState.enabled` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/Notifications.swift:95 NotificationState.failed` | case | `NotificationState.failed` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:3 ConsentState` | enum | `ConsentState` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:4 ConsentState.allowed` | case | `ConsentState.allowed` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:4 ConsentState.denied` | case | `ConsentState.denied` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:4 ConsentState.unknown` | case | `ConsentState.unknown` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:7 EntryType` | enum | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:8 EntryType.conversation_id` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:8 EntryType.inbox_id` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:11 PreferenceType` | enum | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:12 PreferenceType.hmac_keys` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:15 ConsentRecord` | struct | `ConsentRecord` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:16 init` | init | `init` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:39 value` | var | `value` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:40 entryType` | var | `entryType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:41 consentType` | var | `consentType` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:49 PrivatePreferences` | actor | `Preferences` | alias | 11.4 Swift; 19.2 | Deprecated name for one major release (19.2; 11.4). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:56 setConsentState` | func | `setConsentState` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:60 conversationState` | func | `conversationState` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:69 inboxIdState` | func | `inboxIdState` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:76 sync` | func | `sync` | static runtime | 11.4 Swift | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:80 streamConsent` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/PrivatePreferences.swift:118 streamPreferenceUpdates` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Proto/*.pb.swift:0 all public declarations` (3715 declarations) | generated family | — | approved removal | 2; 19.34 | Generated SwiftProtobuf files leave the package (2, 19.34). |
| `sdks/ios/Sources/XMTPiOS/SendOptions.swift:10 SendOptions` | struct | `SendOptions` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SendOptions.swift:11 compression` | var | `compression` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SendOptions.swift:12 contentType` | var | `contentType` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SendOptions.swift:13 ephemeral` | var | `ephemeral` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SendOptions.swift:16 idempotencyKey` | var | `idempotencyKey` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SendOptions.swift:18 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:17 SigningKeyDescription` | struct | `SigningKeyDescription` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:18 kid` | let | `kid` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:19 alg` | let | `alg` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:29 AuthConfiguration` | struct | `AuthConfiguration` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:30 enabled` | let | `enabled` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:31 keys` | let | `keys` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:32 audiences` | let | `audiences` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:33 issuers` | let | `issuers` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:34 requiredScopes` | let | `requiredScopes` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:46 RetentionConfiguration` | struct | `RetentionConfiguration` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:47 groupMessageSeconds` | let | `groupMessageSeconds` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:48 welcomeSeconds` | let | `welcomeSeconds` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:49 keyPackageSeconds` | let | `keyPackageSeconds` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:60 LimitsConfiguration` | struct | `LimitsConfiguration` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:61 maxEnvelopeBytes` | let | `maxEnvelopeBytes` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:62 maxRequestBytes` | let | `maxRequestBytes` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:63 maxResponseBytes` | let | `maxResponseBytes` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:64 maxPublishTopics` | let | `maxPublishTopics` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:65 maxQueryTopics` | let | `maxQueryTopics` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:66 maxQueryLimit` | let | `maxQueryLimit` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:67 defaultQueryLimit` | let | `defaultQueryLimit` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:68 maxNewestMetadataTopics` | let | `maxNewestMetadataTopics` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:69 maxNewestFullTopics` | let | `maxNewestFullTopics` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:70 maxUpdateAdds` | let | `maxUpdateAdds` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:71 maxUpdateRemoves` | let | `maxUpdateRemoves` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:72 maxStreamTopics` | let | `maxStreamTopics` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:73 maxStaticTopics` | let | `maxStaticTopics` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:74 maxLookupIdentifiers` | let | `maxLookupIdentifiers` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:75 maxScwSignatures` | let | `maxScwSignatures` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:76 maxIdentityEntries` | let | `maxIdentityEntries` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:77 maxUpdateFramesPerSecond` | let | `maxUpdateFramesPerSecond` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:78 maxUpdateBurst` | let | `maxUpdateBurst` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:79 maxPingFramesPerSecond` | let | `maxPingFramesPerSecond` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:80 maxPingBurst` | let | `maxPingBurst` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:107 MlsConfiguration` | struct | `MlsConfiguration` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:108 maxGroupMembers` | let | `maxGroupMembers` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:109 maxInstallationsPerInbox` | let | `maxInstallationsPerInbox` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:112 commitLogEnabled` | let | `commitLogEnabled` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:122 ServerConfiguration` | struct | `ServerConfiguration` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:124 identifier` | let | `identifier` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:125 serverVersion` | let | `serverVersion` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:127 minLibxmtpVersion` | let | `minLibxmtpVersion` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:128 auth` | let | `auth` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:129 retention` | let | `retention` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:130 limits` | let | `limits` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:131 mls` | let | `mls` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfiguration.swift:134 smartContractWalletChains` | let | `smartContractWalletChains` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:18 ServerConfigurationError` | protocol | `ServerConfigurationError` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:26 ServerConfigurationError.description` | var | `ServerConfigurationError.description` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:30 ServerConfigurationError.errorDescription` | var | `ServerConfigurationError.errorDescription` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:37 ConfigurationUnavailableError` | struct | `ConfigurationUnavailableError` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:40 message` | let | `message` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:44 ConfigurationInvalidError` | struct | `ConfigurationInvalidError` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:47 message` | let | `message` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:52 BackendMismatchError` | struct | `BackendMismatchError` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:53 message` | let | `message` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:59 ClientVersionTooOldError` | struct | `ClientVersionTooOldError` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:62 message` | let | `message` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:67 AuthRequiredError` | struct | `AuthRequiredError` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:68 message` | let | `message` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:74 ChainNotAcceptedError` | struct | `ChainNotAcceptedError` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:75 message` | let | `message` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/ServerConfigurationError.swift:88 Error.serverConfigurationError` | var | `Error.serverConfigurationError` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SignedData.swift:3 SignedData` | struct | `Signature` | generated | 11.4 Swift | Signer contract changes shape (11.1, 11.4). |
| `sdks/ios/Sources/XMTPiOS/SignedData.swift:5 rawData` | let | `rawData` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SignedData.swift:8 publicKey` | let | `publicKey` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SignedData.swift:11 authenticatorData` | let | `authenticatorData` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SignedData.swift:14 clientDataJson` | let | `clientDataJson` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SignedData.swift:16 init` | init | `init` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SigningKey.swift:3 SignerType` | enum | `SignerKind` | generated | 11.4 Swift | Signer contract changes shape (11.1, 11.4). |
| `sdks/ios/Sources/XMTPiOS/SigningKey.swift:4 SignerType.EOA` | case | `SignerType.EOA` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/SigningKey.swift:4 SignerType.SCW` | case | `SignerType.SCW` | generated | 11.4 Swift | Enum case or generated value (11.1-11.2). |
| `sdks/ios/Sources/XMTPiOS/SigningKey.swift:8 SigningKey` | protocol | `Signer` | generated | 11.4 Swift | Signer contract changes shape (11.1, 11.4). |
| `sdks/ios/Sources/XMTPiOS/SigningKey.swift:10 SigningKey.identity` | var | `SigningKey.identity` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SigningKey.swift:13 SigningKey.type` | var | `SigningKey.type` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SigningKey.swift:16 SigningKey.chainId` | var | `SigningKey.chainID` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SigningKey.swift:19 SigningKey.blockNumber` | var | `SigningKey.blockNumber` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SigningKey.swift:22 SigningKey.sign` | func | `SigningKey.sign` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SigningKey.swift:27 SigningKey.type` | var | `SigningKey.type` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SigningKey.swift:31 SigningKey.chainId` | var | `SigningKey.chainID` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/SigningKey.swift:35 SigningKey.blockNumber` | var | `SigningKey.blockNumber` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/StreamFailure.swift:3 StreamFailureKind` | typealias | `StreamFailureKind` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/StreamFailure.swift:4 StreamBarrierReason` | typealias | `StreamBarrierReason` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/StreamFailure.swift:5 StreamBarrierCauseKind` | typealias | `StreamBarrierCauseKind` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/StreamFailure.swift:6 StreamBarrierCause` | typealias | `StreamBarrierCause` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/StreamFailure.swift:7 StreamBarrierTopic` | typealias | `StreamBarrierTopic` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/StreamFailure.swift:8 StreamBarrierFailure` | typealias | `StreamBarrierFailure` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/StreamFailure.swift:9 StreamFailureDetails` | typealias | `StreamFailureDetails` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/StreamFailure.swift:15 Error.streamFailureDetails` | var | `Error.streamFailureDetails` | generated | 11.4 Swift | Facade schema or generated record (11.1-11.4). |
| `sdks/ios/Sources/XMTPiOS/StreamLifecycle.swift:10 CatchUpSummary` | struct | `CatchUpSummary` | platform helper | 2 | Native OS integration stays under sdks/ (2). |
| `sdks/ios/Sources/XMTPiOS/StreamLifecycle.swift:12 messages` | let | `messages` | platform helper | 2 | Native OS integration stays under sdks/ (2). |
| `sdks/ios/Sources/XMTPiOS/StreamLifecycle.swift:14 conversations` | let | `conversations` | platform helper | 2 | Native OS integration stays under sdks/ (2). |
| `sdks/ios/Sources/XMTPiOS/StreamLifecycle.swift:16 failed` | let | `failed` | platform helper | 2 | Native OS integration stays under sdks/ (2). |
| `sdks/ios/Sources/XMTPiOS/StreamLifecycle.swift:18 completed` | let | `completed` | platform helper | 2 | Native OS integration stays under sdks/ (2). |
| `sdks/ios/Sources/XMTPiOS/Topic.swift:8 Topic` | enum | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/Topic.swift:9 Topic.userWelcome` | case | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/UnstableChangeCallbacks.swift:4 AppDataChange` | struct | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/UnstableChangeCallbacks.swift:6 groupId` | let | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/UnstableChangeCallbacks.swift:8 oldValue` | let | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/UnstableChangeCallbacks.swift:10 newValue` | let | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/UnstableChangeCallbacks.swift:26 AppDataChangeHandler` | protocol | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/UnstableChangeCallbacks.swift:27 AppDataChangeHandler.onAppDataChanged` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/UnstableChangeCallbacks.swift:43 UnstableChangeCallbacks` | struct | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/UnstableChangeCallbacks.swift:44 appData` | var | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/UnstableChangeCallbacks.swift:46 init` | init | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/UnstableGroup.swift:35 enableProposals` | func | — | approved removal | 11.4 Swift | Removal or replacement approved in 11.4 and 19. |
| `sdks/ios/Sources/XMTPiOS/XMTPLogger.swift:7 XMTPLogger` | enum | `XMTPLogger` | platform helper | 2 | Native OS integration stays under sdks/ (2). |
| `sdks/ios/Sources/XMTPiOS/XMTPLogger.swift:9 main` | let | `main` | platform helper | 2 | Native OS integration stays under sdks/ (2). |
| `sdks/ios/Sources/XMTPiOS/XMTPLogger.swift:12 database` | let | `database` | platform helper | 2 | Native OS integration stays under sdks/ (2). |

## Kotlin

| Current export | Kind | Final name | Status | Design ref | Notes |
| --- | --- | --- | --- | --- | --- |
| `sdks/android/library/src/main/java/org/xmtp/android/library/BackendAuth.kt:9 AuthCallback` | typealias | `AuthCallback` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/BackendAuth.kt:16 Credential` | class | `Credential` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/BackendAuth.kt:17 value` | constructor property | `value` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/BackendAuth.kt:18 expiresAtSeconds` | constructor property | `expiresAtSeconds` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/BackendAuth.kt:19 name` | constructor property | `name` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/BackendAuth.kt:21 toString` | fun | `toString` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:49 PreEventCallback` | typealias | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:50 ProcessType` | typealias | `ProcessType` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:51 MessageMetadata` | typealias | `MessageMetadata` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:53 ClientOptions` | class | `ClientOptions` | static runtime | 11.4 Kotlin | Host options wrapper accepts codecs or callbacks (11.1, 20.1). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:54 api` | constructor property | `backend` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:55 preAuthenticateToInboxCallback` | constructor property | `handlers.preAuthenticate` | static runtime | 11.4 Kotlin | Host options wrapper accepts codecs or callbacks (11.1, 20.1). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:56 appContext` | constructor property | `StorageOptions(context)` | platform helper | 2 | Android Context overload stays native (2, 19.26). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:57 dbEncryptionKey` | constructor property | `storage.encryptionKey` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:58 dbDirectory` | constructor property | `storage.location.directory` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:59 deviceSyncEnabled` | constructor property | `deviceSync` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:60 forkRecoveryOptions` | constructor property | `forkRecovery` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:61 dbPoolOptions` | constructor property | `storage.pool` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:62 waitForRegistrationVisible` | constructor property | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:67 unstableChangeCallbacks` | constructor property | — | approved removal | 11.4 Kotlin | Debug events or unstable callbacks leave ClientOptions (11.4, 19.31/35). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:69 Api` | class | `BackendOptions` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:70 backendUrl` | constructor property | `backend.url` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:71 env` | constructor property | `storage.label` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:72 appVersion` | constructor property | `appVersion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:73 authCallback` | constructor property | `backend.credentials` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:91 ForkRecoveryPolicy` | class | `ForkRecoveryPolicy` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:92 None` | enum case | `None` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:93 AllowlistedGroups` | enum case | `AllowlistedGroups` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:94 All` | enum case | `All` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:97 toFfi` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:98 when` | enum case | `when` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:105 ForkRecoveryOptions` | class | `ForkRecoveryOptions` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:106 enableRecoveryRequests` | constructor property | `enableRecoveryRequests` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:107 groupsToRequestRecovery` | constructor property | `groupsToRequestRecovery` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:108 disableRecoveryResponses` | constructor property | `disableRecoveryResponses` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:109 workerIntervalNs` | constructor property | `workerIntervalNs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:111 toFfi` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:120 VisibilityConfirmationOptions` | class | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:121 timeoutMs` | constructor property | `timeoutMs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:123 toFfi` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:129 DbPoolOptions` | class | `DbPoolOptions` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:130 maxPoolSize` | constructor property | `maxPoolSize` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:131 minPoolSize` | constructor property | `minPoolSize` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:134 InboxId` | typealias | `InboxID` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:136 Client` | class | `Client` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:138 dbPath` | constructor property | `storage.path` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:139 installationId` | constructor property | `installationID` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:140 inboxId` | constructor property | `inboxID` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:141 environment` | constructor property | `options.storage.label` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:142 publicIdentity` | constructor property | `identity` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:144 preferences` | val | `preferences` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:146 conversations` | val | `conversations` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:152 debugInformation` | val | `diagnostics` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:154 libXMTPVersion` | val | `libxmtpVersion` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:158 enableNotifications` | fun | `enableNotifications` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:168 disableNotifications` | fun | `disableNotifications` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:178 notificationState` | fun | `notificationState` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:187 isInMemory` | val | `isInMemory` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:190 Companion` | object | `Companion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:204 manageStreamLifecycle` | var | `manageStreamLifecycle` | platform helper | 2 | Native lifecycle or log writer helper (2, 19.26/35). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:210 IN_MEMORY_DB_PATH` | val | `IN_MEMORY_DB_PATH` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:212 codecRegistry` | var | — | approved removal | 11.4 Kotlin | Global registry becomes per-client codecs (11.4, 19.9). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:222 activatePersistentLibXMTPLogWriter` | fun | `activatePersistentLibXMTPLogWriter` | platform helper | 2 | Native lifecycle or log writer helper (2, 19.26/35). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:242 deactivatePersistentLibXMTPLogWriter` | fun | `deactivatePersistentLibXMTPLogWriter` | platform helper | 2 | Native lifecycle or log writer helper (2, 19.26/35). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:251 setLibXMTPNativeLogLevel` | fun | `setLibXMTPNativeLogLevel` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:255 getXMTPLogFilePaths` | fun | `getXMTPLogFilePaths` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:268 clearXMTPLogs` | fun | `clearXMTPLogs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:290 connectToApiBackend` | fun | `Backend.connect()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:323 getOrCreateInboxId` | fun | `Client.inboxID()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:340 revokeInstallations` | fun | `revokeInstallations` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:357 ffiRevokeInstallations` | fun | `unsafeRevokeInstallationsSignatureRequest()` | alias | 11.4 Kotlin; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:374 ffiApplySignatureRequest` | fun | `unsafeApplySignatureRequest()` | alias | 11.4 Kotlin; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:382 register` | fun | — | approved removal | 11.4 Kotlin | Global codec registration becomes ClientOptions.codecs (11.4, 19.9). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:424 inboxStatesForInboxIds` | fun | `inboxStatesForInboxIDs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:445 fetchServerConfiguration` | fun | `fetchServerConfiguration` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:454 fetchServerConfiguration` | fun | `fetchServerConfiguration` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:457 getNewestMessageMetadata` | fun | `getNewestMessageMetadata` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:470 keyPackageStatusesForInstallationIds` | fun | `keyPackageStatusesForInstallationIDs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:484 canMessage` | fun | `canMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:570 create` | fun | `create` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:600 createInMemory` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:618 build` | fun | `build` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:747 ffiCreateClient` | fun | `Client.build()` | alias | 11.4 Kotlin; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:777 revokeInstallations` | fun | `revokeInstallations` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:787 revokeAllOtherInstallations` | fun | `revokeAllOtherInstallations` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:798 addAccount` | fun | `addAccount` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:807 removeAccount` | fun | `removeAccount` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:816 signWithInstallationKey` | fun | `signWithInstallationKey` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:818 verifySignature` | fun | `verifySignature` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:829 verifySignatureWithInstallationId` | fun | `verifySignatureWithInstallationID` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:841 canMessage` | fun | `canMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:851 inboxIdFromIdentity` | fun | `inboxIdFromIdentity` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:856 deleteLocalDatabase` | fun | `storage.delete()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:867 dropLocalDatabaseConnection` | fun | `end()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:875 reconnectLocalDatabase` | fun | `storage.reconnect()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:897 catchUpToLive` | fun | `catchUpToLive` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:906 inboxStatesForInboxIds` | fun | `inboxStatesForInboxIDs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:916 inboxState` | fun | `inboxState` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:930 serverConfiguration` | fun | `serverConfiguration` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:943 refreshServerConfiguration` | fun | `refreshServerConfiguration` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:951 syncAllDeviceSyncGroups` | fun | `syncAllDeviceSyncGroups` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:956 createArchive` | fun | `archives.exportToFile()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:964 importArchive` | fun | `archives.importFromFile()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:971 archiveMetadata` | fun | `archives.metadataFromFile()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:982 ffiApplySignatureRequest` | fun | `unsafeApplySignatureRequest()` | alias | 11.4 Kotlin; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:989 ffiRevokeInstallations` | fun | `unsafeRevokeInstallationsSignatureRequest()` | alias | 11.4 Kotlin; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:995 ffiRevokeAllOtherInstallations` | fun | `unsafeRevokeAllOtherInstallationsSignatureRequest()` | alias | 11.4 Kotlin; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:1001 ffiRevokeIdentity` | fun | `unsafeRemoveAccountSignatureRequest()` | alias | 11.4 Kotlin; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:1007 ffiAddIdentity` | fun | `unsafeAddAccountSignatureRequest()` | alias | 11.4 Kotlin; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:1033 ffiSignatureRequest` | fun | `unsafeCreateInboxSignatureRequest()` | alias | 11.4 Kotlin; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:1038 ffiRegisterIdentity` | fun | `register()` | alias | 11.4 Kotlin; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/CodecRegistry.kt:8 CodecRegistry` | class | `CodecRegistry` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/CodecRegistry.kt:9 codecs` | constructor property | `codecs` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/CodecRegistry.kt:11 register` | fun | `register` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/CodecRegistry.kt:16 find` | fun | `find` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/CodecRegistry.kt:26 findFromId` | fun | `findFromID` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:18 Conversation` | class | `Conversation` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:19 Group` | class | `Group` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:20 group` | constructor property | `group` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:23 Dm` | class | `Dm` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:24 dm` | constructor property | `dm` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:27 Type` | class | `Type` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:28 GROUP` | enum case | `GROUP` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:29 DM` | enum case | `DM` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:32 type` | val | `type` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:40 id` | val | `id` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:48 topic` | val | `topic` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:56 createdAt` | val | `createdAt` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:64 createdAtNs` | val | `createdAt.ns` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:72 lastActivityNs` | val | `lastActivityAtNs(contentTypes?)` | generated | 11.4 Kotlin | Optional content-type filter; outside state() (plan Decisions, 19.46). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:84 disappearingMessageSettings` | val | — | approved removal | 11.4 Kotlin | Deprecated blocking property is removed; read state() (11.4 Kotlin, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:92 disappearingMessageSettings` | fun | `state().disappearingSettings` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:104 isDisappearingMessagesEnabled` | val | — | approved removal | 11.4 Kotlin | Deprecated blocking property is removed; read state() (11.4 Kotlin, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:112 isDisappearingMessagesEnabled` | fun | `state().isDisappearingEnabled` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:120 lastMessage` | fun | `lastMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:128 commitLogForkStatus` | fun | `state().commitLogForkStatus` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:134 members` | fun | `members` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:142 clearDisappearingMessageSettings` | fun | `clearDisappearingMessageSettings` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:150 updateDisappearingMessageSettings` | fun | `updateDisappearingMessageSettings` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:158 updateConsentState` | fun | `updateConsentState` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:166 consentState` | fun | `state().consentState` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:181 prepareMessage` | fun | `prepareMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:200 prepareMessage` | fun | `prepareMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:212 send` | fun | `send` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:223 send` | fun | `send` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:234 send` | fun | `send` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:251 deleteMessage` | fun | `deleteMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:259 sync` | fun | `sync` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:279 messages` | fun | `messages` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:326 countMessages` | fun | `countMessages` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:380 enrichedMessages` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:427 processMessage` | fun | `processMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:435 publishMessages` | fun | `publishMessages` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:447 publishMessage` | fun | `publishMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:457 pausedForVersion` | fun | `state().pausedForVersion` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:465 client` | val | `client` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:473 messageReader` | fun | `messageReader` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:479 messageHistorySnapshot` | fun | `messageHistorySnapshot` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:485 beginningDeliveryCursor` | fun | `beginningDeliveryCursor` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:492 streamMessages` | fun | `stream()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:498 getHmacKeys` | fun | `hmacKeys()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:507 setNotifications` | fun | `setNotifications` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:516 notificationsEnabled` | fun | `state().notificationsEnabled` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:524 getDebugInformation` | fun | `debugInfo()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:532 isActive` | fun | `state().isActive` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversation.kt:542 getLastReadTimes` | fun | `lastReadTimes()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:36 GroupSyncSummary` | class | `GroupSyncSummary` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:37 numEligible` | constructor property | `numEligible` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:38 numSynced` | constructor property | `numSynced` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:40 Companion` | object | `Companion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:41 fromFfi` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:48 toFfi` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:55 Conversations` | class | `Conversations` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:56 client` | constructor property | `client` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:60 ConversationFilterType` | class | `ConversationKind` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:61 ALL` | enum case | `ALL` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:62 GROUPS` | enum case | `GROUPS` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:63 DMS` | enum case | `DMS` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:66 ListConversationsOrderBy` | class | `ListConversationsOrderBy` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:67 CREATED_AT` | enum case | `CREATED_AT` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:68 LAST_ACTIVITY` | enum case | `LAST_ACTIVITY` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:77 findGroup` | fun | `getByID()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:86 findConversation` | fun | `getByID()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:100 findConversationByTopic` | fun | `getByID()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:117 findDmByInboxId` | fun | `getDmByInboxID()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:126 findDmByIdentity` | fun | `getDmByIdentity()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:134 findMessage` | fun | `getMessageByID()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:143 findEnrichedMessage` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:153 fromWelcome` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:164 newGroupWithIdentities` | fun | `newGroupWithIdentities` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:192 newGroupCustomPermissionsWithIdentities` | fun | `newGroupCustomPermissionsWithIdentities` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:248 newGroup` | fun | `createGroup()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:276 newGroupCustomPermissions` | fun | `newGroupCustomPermissions` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:333 newGroupOptimistic` | fun | `newGroupOptimistic` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:370 sync` | fun | `sync` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:373 syncAllConversations` | fun | `syncAll()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:384 newConversationWithIdentity` | fun | `newConversationWithIdentity` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:393 findOrCreateDmWithIdentity` | fun | `findOrCreateDmWithIdentity` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:420 newConversation` | fun | `createDm()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:429 findOrCreateDm` | fun | `findOrCreateDm` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:454 listGroups` | fun | `listGroups` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:487 listDms` | fun | `listDms` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:520 list` | fun | `list` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:580 stream` | fun | `stream` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:652 messageReader` | fun | `messageReader` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:667 messageHistorySnapshot` | fun | `messageHistorySnapshot` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:683 beginningDeliveryCursor` | fun | `beginningDeliveryCursor` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:687 streamAllMessages` | fun | `streamAllMessages` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:720 streamMessageDeletions` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:740 getHmacKeys` | fun | `hmacKeys()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Conversations.kt:761 deleteMessageLocally` | fun | `deleteMessageLocally` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Crypto.kt:13 CipherText` | typealias | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Crypto.kt:15 Crypto` | class | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Crypto.kt:16 Companion` | object | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Crypto.kt:19 encrypt` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Crypto.kt:56 decrypt` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/DelicateApi.kt:6 DelicateApi` | class | `DelicateApi` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/DelicateApi.kt:7 message` | constructor property | `message` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:31 Dm` | class | `Dm` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:32 client` | constructor property | `client` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:37 id` | val | `id` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:40 topic` | val | `topic` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:43 createdAt` | val | `createdAt` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:46 createdAtNs` | val | `createdAt.ns` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:49 lastActivityNs` | val | `lastActivityAtNs(contentTypes?)` | generated | 11.4 Kotlin | Optional content-type filter; outside state() (plan Decisions, 19.46). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:52 peerInboxId` | val | `peerInboxID` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:59 disappearingMessageSettings` | val | — | approved removal | 11.4 Kotlin | Deprecated blocking property is removed; read state() (11.4 Kotlin, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:69 disappearingMessageSettings` | fun | `state().disappearingSettings` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:83 isDisappearingMessagesEnabled` | val | — | approved removal | 11.4 Kotlin | Deprecated blocking property is removed; read state() (11.4 Kotlin, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:86 isDisappearingMessagesEnabled` | fun | `state().isDisappearingEnabled` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:92 send` | fun | `send` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:98 send` | fun | `send` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:107 send` | fun | `send` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:120 encodeContent` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:160 prepareMessage` | fun | `prepareMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:180 prepareMessage` | fun | `prepareMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:190 publishMessages` | fun | `publishMessages` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:196 publishMessage` | fun | `publishMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:208 deleteMessage` | fun | `deleteMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:217 sync` | fun | `sync` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:219 lastMessage` | fun | `lastMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:228 commitLogForkStatus` | fun | `state().commitLogForkStatus` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:247 messages` | fun | `messages` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:309 countMessages` | fun | `countMessages` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:370 enrichedMessages` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:432 processMessage` | fun | `processMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:439 creatorInboxId` | fun | `creatorInboxID` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:441 isCreator` | fun | `isCreator` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:443 isActive` | fun | `state().isActive` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:445 members` | fun | `members` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:448 messageReader` | fun | `messageReader` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:451 messageHistorySnapshot` | fun | `messageHistorySnapshot` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:454 beginningDeliveryCursor` | fun | `beginningDeliveryCursor` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:458 streamMessages` | fun | `stream()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:463 clearDisappearingMessageSettings` | fun | `clearDisappearingMessageSettings` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:475 updateDisappearingMessageSettings` | fun | `updateDisappearingMessageSettings` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:496 updateConsentState` | fun | `updateConsentState` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:502 consentState` | fun | `state().consentState` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:508 pausedForVersion` | fun | `state().pausedForVersion` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:510 getHmacKeys` | fun | `hmacKeys()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:532 setNotifications` | fun | `setNotifications` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:536 notificationsEnabled` | fun | `state().notificationsEnabled` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:538 getDebugInformation` | fun | `debugInfo()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:543 getLastReadTimes` | fun | `lastReadTimes()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:545 equals` | fun | `equals` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Dm.kt:554 hashCode` | fun | `hashCode` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/EncodedContentCompression.kt:10 EncodedContentCompression` | class | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/EncodedContentCompression.kt:11 DEFLATE` | enum case | `DEFLATE` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/EncodedContentCompression.kt:12 GZIP` | enum case | `GZIP` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/EncodedContentCompression.kt:15 compress` | fun | `compress` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/EncodedContentCompression.kt:16 when` | enum case | `when` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/EncodedContentCompression.kt:38 decompress` | fun | `decompress` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/EncodedContentCompression.kt:39 when` | enum case | `when` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:43 Group` | class | `Group` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:44 client` | constructor property | `client` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:49 id` | val | `id` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:52 topic` | val | `topic` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:55 createdAt` | val | `createdAt` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:58 createdAtNs` | val | `createdAt.ns` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:61 lastActivityNs` | val | `lastActivityAtNs(contentTypes?)` | generated | 11.4 Kotlin | Optional content-type filter; outside state() (plan Decisions, 19.46). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:67 permissions` | fun | `state().permissions` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:73 name` | val | — | approved removal | 11.4 Kotlin | Deprecated blocking property is removed; read state() (11.4 Kotlin, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:76 name` | fun | `state().name` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:82 imageUrl` | val | — | approved removal | 11.4 Kotlin | Deprecated blocking property is removed; read state() (11.4 Kotlin, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:85 imageUrl` | fun | `state().imageUrl` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:91 description` | val | — | approved removal | 11.4 Kotlin | Deprecated blocking property is removed; read state() (11.4 Kotlin, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:94 description` | fun | `state().description` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:100 appData` | val | — | approved removal | 11.4 Kotlin | Deprecated blocking property is removed; read state() (11.4 Kotlin, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:103 appData` | fun | `state().appData` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:109 disappearingMessageSettings` | val | — | approved removal | 11.4 Kotlin | Deprecated blocking property is removed; read state() (11.4 Kotlin, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:119 disappearingMessageSettings` | fun | `state().disappearingSettings` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:133 isDisappearingMessagesEnabled` | val | — | approved removal | 11.4 Kotlin | Deprecated blocking property is removed; read state() (11.4 Kotlin, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:136 isDisappearingMessagesEnabled` | fun | `state().isDisappearingEnabled` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:139 send` | fun | `send` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:145 send` | fun | `send` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:154 send` | fun | `send` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:167 encodeContent` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:207 prepareMessage` | fun | `prepareMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:227 prepareMessage` | fun | `prepareMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:237 publishMessages` | fun | `publishMessages` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:243 publishMessage` | fun | `publishMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:255 deleteMessage` | fun | `deleteMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:264 sync` | fun | `sync` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:266 lastMessage` | fun | `lastMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:275 commitLogForkStatus` | fun | `state().commitLogForkStatus` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:294 messages` | fun | `messages` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:372 enrichedMessages` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:434 processMessage` | fun | `processMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:441 updateConsentState` | fun | `updateConsentState` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:447 consentState` | fun | `state().consentState` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:452 isActive` | fun | `state().isActive` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:454 membershipState` | fun | `state().membershipState` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:459 addedByInboxId` | fun | `addedByInboxID` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:461 permissionPolicySet` | fun | `state().permissions.policySet` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:466 creatorInboxId` | fun | `creatorInboxID` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:468 isCreator` | fun | `isCreator` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:470 addMembersByIdentity` | fun | `addMembersByIdentity` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:480 removeMembersByIdentity` | fun | `removeMembersByIdentity` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:489 addMembers` | fun | `addMembers` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:500 removeMembers` | fun | `removeMembers` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:510 members` | fun | `members` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:512 peerInboxIds` | fun | `peerInboxIDs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:519 updateName` | fun | `updateName` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:528 updateImageUrl` | fun | `updateImageUrl` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:537 updateDescription` | fun | `updateDescription` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:557 updateAppData` | fun | `updateAppData` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:580 unstable` | val | `unstable` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:589 proposalsEnabled` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:612 membershipCapabilities` | fun | `membershipCapabilities` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:621 clearDisappearingMessageSettings` | fun | `clearDisappearingMessageSettings` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:633 updateDisappearingMessageSettings` | fun | `updateDisappearingMessageSettings` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:654 updateAddMemberPermission` | fun | `updateAddMemberPermission` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:663 updateRemoveMemberPermission` | fun | `updateRemoveMemberPermission` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:672 updateAddAdminPermission` | fun | `updateAddAdminPermission` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:681 updateRemoveAdminPermission` | fun | `updateRemoveAdminPermission` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:690 updateNamePermission` | fun | `updateNamePermission` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:699 updateDescriptionPermission` | fun | `updateDescriptionPermission` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:708 updateImageUrlPermission` | fun | `updateImageUrlPermission` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:717 isAdmin` | fun | `isAdmin` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:719 isSuperAdmin` | fun | `isSuperAdmin` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:722 addAdmin` | fun | `addAdmin` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:731 removeAdmin` | fun | `removeAdmin` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:740 addSuperAdmin` | fun | `addSuperAdmin` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:749 removeSuperAdmin` | fun | `removeSuperAdmin` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:758 listAdmins` | fun | `listAdmins` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:760 listSuperAdmins` | fun | `listSuperAdmins` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:763 pausedForVersion` | fun | `state().pausedForVersion` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:766 messageReader` | fun | `messageReader` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:769 messageHistorySnapshot` | fun | `messageHistorySnapshot` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:772 beginningDeliveryCursor` | fun | `beginningDeliveryCursor` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:776 streamMessages` | fun | `stream()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:781 getHmacKeys` | fun | `hmacKeys()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:802 countMessages` | fun | `countMessages` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:848 setNotifications` | fun | `setNotifications` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:852 notificationsEnabled` | fun | `state().notificationsEnabled` | generated | 11.4 Kotlin | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:854 getDebugInformation` | fun | `debugInfo()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:859 getLastReadTimes` | fun | `lastReadTimes()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:861 leaveGroup` | fun | `requestRemoval()` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:863 equals` | fun | `equals` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Group.kt:872 hashCode` | fun | `hashCode` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/KeyUtil.kt:7 KeyUtil` | object | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/KeyUtil.kt:10 ethHash` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/KeyUtil.kt:15 getPublicKey` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/KeyUtil.kt:18 addUncompressedByte` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/KeyUtil.kt:31 getSignatureData` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/KeyUtil.kt:41 getSignatureBytes` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/MessageDeliveryFlow.kt:18 decode` | constructor property | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/MessageDeliveryFlow.kt:19 checkOwner` | val | `checkOwner` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/MessageDeliveryFlow.kt:20 acknowledge` | val | `acknowledge` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/MessageDeliveryFlow.kt:21 reject` | val | `reject` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/MessageReader.kt:16 DeliveryCursor` | typealias | `DeliveryCursor` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/MessageReader.kt:17 MessageCatchUpSnapshot` | typealias | `MessageCatchUpSnapshot` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/MessageReader.kt:19 MessageHistorySnapshot` | class | `MessageHistorySnapshot` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/MessageReader.kt:20 messages` | constructor property | `messages` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/MessageReader.kt:21 cursor` | constructor property | `cursor` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:10 NotificationChannel` | class | `NotificationChannel` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:11 Apns` | class | `Apns` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:12 token` | constructor property | `token` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:15 Fcm` | class | `Fcm` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:16 token` | constructor property | `token` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:19 Http` | class | `Http` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:20 url` | constructor property | `url` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:21 signingKey` | constructor property | `signingKey` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:33 NotificationConfig` | class | `NotificationConfig` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:34 channel` | constructor property | `channel` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:35 consentStates` | constructor property | `consentStates` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:36 includeWelcomes` | val | `includeWelcomes` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:37 includeSyncGroups` | val | `includeSyncGroups` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:38 includeCommits` | val | `includeCommits` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:51 NotificationOverride` | class | `NotificationOverride` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:52 Enabled` | enum case | `Enabled` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:53 Disabled` | enum case | `Disabled` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:54 Default` | enum case | `Default` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:58 when` | enum case | `when` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:67 code` | constructor property | `code` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:91 NotificationState` | class | `NotificationState` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:92 Disabled` | object | `Disabled` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:94 Enabled` | object | `Enabled` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:96 Failed` | class | `Failed` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:97 error` | constructor property | `error` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Notifications.kt:100 Companion` | object | `Companion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:16 ConsentState` | class | `ConsentState` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:17 ALLOWED` | enum case | `ALLOWED` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:18 DENIED` | enum case | `DENIED` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:19 UNKNOWN` | enum case | `UNKNOWN` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:22 Companion` | object | `Companion` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:23 toFfiConsentState` | fun | `toFfiConsentState` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:30 fromFfiConsentState` | fun | `fromFfiConsentState` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:39 EntryType` | class | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:40 CONVERSATION_ID` | enum case | `CONVERSATION_ID` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:41 INBOX_ID` | enum case | `INBOX_ID` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:44 Companion` | object | `Companion` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:45 toFfiConsentEntityType` | fun | `toFfiConsentEntityType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:51 fromFfiConsentEntityType` | fun | `fromFfiConsentEntityType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:59 PreferenceType` | class | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:60 HMAC_KEYS` | enum case | `HMAC_KEYS` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:63 ConsentRecord` | class | `ConsentRecord` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:64 value` | constructor property | `value` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:65 entryType` | constructor property | `entryType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:66 consentType` | constructor property | `consentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:68 Companion` | object | `Companion` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:69 conversationId` | fun | `conversationID` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:74 inboxId` | fun | `inboxID` | alias | 11.4 Kotlin; 19.2 | Deprecated name for one major release (19.2; 11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:80 key` | val | `key` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:84 PrivatePreferences` | class | `Preferences` | alias | 11.4 Kotlin; 19.2 | Deprecated name for one major release (19.2; 11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:85 client` | constructor property | `client` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:88 sync` | fun | `sync` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:93 syncConsent` | fun | `syncConsent` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:97 streamPreferenceUpdates` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:124 streamConsent` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:149 setConsentState` | fun | `setConsentState` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:167 conversationState` | fun | `conversationState` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/PrivatePreferences.kt:175 inboxIdState` | fun | `inboxIdState` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SendOptions.kt:6 SendOptions` | class | `SendOptions` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SendOptions.kt:7 compression` | constructor property | `compression` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SendOptions.kt:8 contentType` | constructor property | `contentType` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SendOptions.kt:10 ephemeral` | var | `ephemeral` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SendOptions.kt:13 idempotencyKey` | var | `idempotencyKey` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SendOptions.kt:16 MessageVisibilityOptions` | class | `MessageVisibilityOptions` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SendOptions.kt:17 shouldPush` | constructor property | `shouldPush` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SendOptions.kt:20 idempotencyKey` | constructor property | `idempotencyKey` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SendOptions.kt:22 toFfi` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:22 ConfigurationUnavailableException` | typealias | `ConfigurationUnavailableException` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:25 ConfigurationInvalidException` | typealias | `ConfigurationInvalidException` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:28 BackendMismatchException` | typealias | `BackendMismatchException` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:31 ClientVersionTooOldException` | typealias | `ClientVersionTooOldException` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:34 AuthRequiredException` | typealias | `AuthRequiredException` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:37 ChainNotAcceptedException` | typealias | `ChainNotAcceptedException` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:40 SigningKeyDescription` | class | `SigningKeyDescription` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:41 kid` | constructor property | `kid` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:42 alg` | constructor property | `alg` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:54 AuthConfiguration` | class | `AuthConfiguration` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:55 enabled` | constructor property | `enabled` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:56 keys` | constructor property | `keys` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:57 audiences` | constructor property | `audiences` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:58 issuers` | constructor property | `issuers` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:59 requiredScopes` | constructor property | `requiredScopes` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:74 RetentionConfiguration` | class | `RetentionConfiguration` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:75 groupMessageSeconds` | constructor property | `groupMessageSeconds` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:76 welcomeSeconds` | constructor property | `welcomeSeconds` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:77 keyPackageSeconds` | constructor property | `keyPackageSeconds` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:93 LimitsConfiguration` | class | `LimitsConfiguration` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:94 maxEnvelopeBytes` | constructor property | `maxEnvelopeBytes` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:95 maxRequestBytes` | constructor property | `maxRequestBytes` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:96 maxResponseBytes` | constructor property | `maxResponseBytes` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:97 maxPublishTopics` | constructor property | `maxPublishTopics` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:98 maxQueryTopics` | constructor property | `maxQueryTopics` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:99 maxQueryLimit` | constructor property | `maxQueryLimit` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:100 defaultQueryLimit` | constructor property | `defaultQueryLimit` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:101 maxNewestMetadataTopics` | constructor property | `maxNewestMetadataTopics` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:102 maxNewestFullTopics` | constructor property | `maxNewestFullTopics` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:103 maxUpdateAdds` | constructor property | `maxUpdateAdds` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:104 maxUpdateRemoves` | constructor property | `maxUpdateRemoves` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:105 maxStreamTopics` | constructor property | `maxStreamTopics` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:106 maxStaticTopics` | constructor property | `maxStaticTopics` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:107 maxLookupIdentifiers` | constructor property | `maxLookupIdentifiers` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:108 maxScwSignatures` | constructor property | `maxScwSignatures` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:109 maxIdentityEntries` | constructor property | `maxIdentityEntries` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:115 maxUpdateFramesPerSecond` | constructor property | `maxUpdateFramesPerSecond` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:116 maxUpdateBurst` | constructor property | `maxUpdateBurst` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:117 maxPingFramesPerSecond` | constructor property | `maxPingFramesPerSecond` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:118 maxPingBurst` | constructor property | `maxPingBurst` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:150 MlsConfiguration` | class | `MlsConfiguration` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:151 maxGroupMembers` | constructor property | `maxGroupMembers` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:152 maxInstallationsPerInbox` | constructor property | `maxInstallationsPerInbox` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:157 commitLogEnabled` | constructor property | `commitLogEnabled` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:184 ServerConfiguration` | class | `ServerConfiguration` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:186 identifier` | constructor property | `identifier` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:187 serverVersion` | constructor property | `serverVersion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:189 minLibxmtpVersion` | constructor property | `minLibxmtpVersion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:190 auth` | constructor property | `auth` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:191 retention` | constructor property | `retention` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:192 limits` | constructor property | `limits` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:193 mls` | constructor property | `mls` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/ServerConfiguration.kt:198 smartContractWalletChains` | constructor property | `smartContractWalletChains` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SignedData.kt:3 SignedData` | class | `Signature` | generated | 11.4 Kotlin | Signer contract changes shape (11.1, 11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SignedData.kt:4 rawData` | constructor property | `rawData` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SignedData.kt:5 publicKey` | constructor property | `publicKey` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SignedData.kt:6 authenticatorData` | constructor property | `authenticatorData` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SignedData.kt:7 clientDataJson` | constructor property | `clientDataJson` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SigningKey.kt:5 SigningKey` | interface | `Signer` | generated | 11.4 Kotlin | Signer contract changes shape (11.1, 11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SigningKey.kt:6 publicIdentity` | val | `identity` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SigningKey.kt:8 type` | val | `type` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SigningKey.kt:12 chainId` | var | `chainID` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SigningKey.kt:17 blockNumber` | var | `blockNumber` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SigningKey.kt:21 sign` | fun | `sign` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SigningKey.kt:24 SignerType` | class | `SignerKind` | generated | 11.4 Kotlin | Signer contract changes shape (11.1, 11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SigningKey.kt:25 SCW` | enum case | `SCW` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/SigningKey.kt:26 EOA` | enum case | `EOA` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamFailure.kt:13 StreamFailureKind` | typealias | `StreamFailureKind` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamFailure.kt:14 StreamBarrierReason` | typealias | `StreamBarrierReason` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamFailure.kt:15 StreamBarrierCauseKind` | typealias | `StreamBarrierCauseKind` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamFailure.kt:16 StreamBarrierCause` | typealias | `StreamBarrierCause` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamFailure.kt:17 StreamBarrierTopic` | typealias | `StreamBarrierTopic` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamFailure.kt:18 StreamBarrierFailure` | typealias | `StreamBarrierFailure` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamFailure.kt:19 StreamFailureDetails` | typealias | `StreamFailureDetails` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamFailure.kt:26 Throwable` | val | `Throwable` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamFailure.kt:28 nativeError` | val | `nativeError` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamLifecycle.kt:23 CatchUpSummary` | class | `CatchUpSummary` | platform helper | 2 | Native OS integration stays under sdks/ (2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamLifecycle.kt:24 messages` | constructor property | `messages` | platform helper | 2 | Native OS integration stays under sdks/ (2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamLifecycle.kt:25 conversations` | constructor property | `conversations` | platform helper | 2 | Native OS integration stays under sdks/ (2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamLifecycle.kt:26 completed` | constructor property | `completed` | platform helper | 2 | Native OS integration stays under sdks/ (2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/StreamLifecycle.kt:27 failed` | constructor property | `failed` | platform helper | 2 | Native OS integration stays under sdks/ (2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Topic.kt:3 Topic` | class | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Topic.kt:5 userWelcome` | class | `userWelcome` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Topic.kt:6 installationId` | constructor property | `installationID` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Topic.kt:10 groupMessage` | class | `groupMessage` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Topic.kt:11 groupId` | constructor property | `groupID` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Topic.kt:14 description` | val | `description` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableApi.kt:14 UnstableApi` | class | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableApi.kt:15 message` | constructor property | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableChangeCallbacks.kt:10 AppDataChange` | class | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableChangeCallbacks.kt:12 groupId` | constructor property | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableChangeCallbacks.kt:14 oldValue` | constructor property | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableChangeCallbacks.kt:16 newValue` | constructor property | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableChangeCallbacks.kt:27 AppDataChangeHandler` | interface | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableChangeCallbacks.kt:28 onAppDataChanged` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableChangeCallbacks.kt:46 UnstableChangeCallbacks` | class | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableChangeCallbacks.kt:47 appData` | constructor property | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableChangeCallbacks.kt:53 toFfi` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableGroup.kt:24 UnstableGroup` | class | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/UnstableGroup.kt:47 enableProposals` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Util.kt:6 Util` | class | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Util.kt:7 Companion` | object | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Util.kt:8 keccak256` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Util.kt:15 ByteArray` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Util.kt:17 String` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Util.kt:19 validateInboxId` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/Util.kt:25 validateInboxIds` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:7 XMTPDebugInformation` | class | `XMTPDebugInformation` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:10 apiStatistics` | val | `apiStatistics` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:12 identityStatistics` | val | `identityStatistics` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:14 aggregateStatistics` | val | `aggregateStatistics` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:17 clearAllStatistics` | fun | `clearAllStatistics` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:20 ApiStats` | class | `ApiStats` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:23 publish` | val | `publish` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:25 query` | val | `query` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:27 queryNewest` | val | `queryNewest` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:29 subscribe` | val | `subscribe` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:31 subscribeStatic` | val | `subscribeStatic` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:35 IdentityStats` | class | `IdentityStats` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:38 getInboxIds` | val | `getInboxIDs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPDebugInformation.kt:40 verifySmartContractWalletSignatures` | val | `verifySmartContractWalletSignatures` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/XMTPException.kt:3 XMTPException` | class | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/AttachmentCodec.kt:6 ContentTypeAttachment` | val | `ContentTypeAttachment` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/AttachmentCodec.kt:14 Attachment` | class | `Attachment` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/AttachmentCodec.kt:15 filename` | constructor property | `filename` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/AttachmentCodec.kt:16 mimeType` | constructor property | `mimeType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/AttachmentCodec.kt:17 data` | constructor property | `data` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/AttachmentCodec.kt:20 AttachmentCodec` | class | `AttachmentCodec` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/AttachmentCodec.kt:21 contentType` | constructor property | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/AttachmentCodec.kt:23 encode` | fun | `encode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/AttachmentCodec.kt:37 decode` | fun | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/AttachmentCodec.kt:49 fallback` | fun | `fallback` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/AttachmentCodec.kt:52 shouldPush` | fun | `shouldPush` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentCodec.kt:10 EncodedContent` | typealias | `EncodedContent` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentCodec.kt:12 EncodedContent` | fun | `EncodedContent` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentCodec.kt:21 EncodedContent` | fun | `EncodedContent` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentCodec.kt:42 EncodedContent` | fun | `EncodedContent` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentCodec.kt:79 encodedContentFromFfi` | fun | `encodedContentFromFfi` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentCodec.kt:96 ContentCodec` | interface | `ContentCodec` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentCodec.kt:97 contentType` | val | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentCodec.kt:99 encode` | fun | `encode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentCodec.kt:101 decode` | fun | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentCodec.kt:103 fallback` | fun | `fallback` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentCodec.kt:105 shouldPush` | fun | `shouldPush` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentCodec.kt:108 id` | val | `id` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentTypeId.kt:5 ContentTypeId` | typealias | `ContentTypeID` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentTypeId.kt:7 ContentTypeIdBuilder` | class | `ContentTypeIdBuilder` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentTypeId.kt:8 Companion` | object | `Companion` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentTypeId.kt:9 builderFromAuthorityId` | fun | `builderFromAuthorityID` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentTypeId.kt:24 fromFfi` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentTypeId.kt:36 ContentTypeId` | val | `ContentTypeID` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ContentTypeId.kt:39 ContentTypeId` | val | `ContentTypeID` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeleteMessageCodec.kt:11 DeleteMessageRequest` | class | `DeleteMessageRequest` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeleteMessageCodec.kt:12 messageId` | constructor property | `messageID` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeleteMessageCodec.kt:15 ContentTypeDeleteMessageRequest` | val | `ContentTypeDeleteMessageRequest` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeleteMessageCodec.kt:23 DeleteMessageCodec` | class | `DeleteMessageCodec` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeleteMessageCodec.kt:24 contentType` | constructor property | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeleteMessageCodec.kt:26 encode` | fun | `encode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeleteMessageCodec.kt:37 decode` | fun | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeleteMessageCodec.kt:45 fallback` | fun | `fallback` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeleteMessageCodec.kt:47 shouldPush` | fun | `shouldPush` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeletedMessage.kt:8 DeletedMessage` | class | `DeletedMessage` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeletedMessage.kt:9 deletedBy` | constructor property | `deletedBy` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeletedMessage.kt:15 DeletedBy` | class | `DeletedBy` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeletedMessage.kt:17 Sender` | object | `Sender` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeletedMessage.kt:20 Admin` | class | `Admin` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/DeletedMessage.kt:21 inboxId` | constructor property | `inboxID` | alias | 11.4 Kotlin; 19.2 | Deprecated name for one major release (19.2; 11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/GroupUpdatedCodec.kt:5 GroupUpdated` | typealias | `GroupUpdated` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/GroupUpdatedCodec.kt:7 ContentTypeGroupUpdated` | val | `ContentTypeGroupUpdated` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/GroupUpdatedCodec.kt:15 GroupUpdatedCodec` | class | `GroupUpdatedCodec` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/GroupUpdatedCodec.kt:16 contentType` | constructor property | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/GroupUpdatedCodec.kt:18 encode` | fun | `encode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/GroupUpdatedCodec.kt:26 decode` | fun | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/GroupUpdatedCodec.kt:28 fallback` | fun | `fallback` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/GroupUpdatedCodec.kt:30 shouldPush` | fun | `shouldPush` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/LeaveRequestCodec.kt:13 LeaveRequest` | class | `LeaveRequest` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/LeaveRequestCodec.kt:14 authenticatedNote` | constructor property | `authenticatedNote` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/LeaveRequestCodec.kt:16 equals` | fun | `equals` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/LeaveRequestCodec.kt:25 hashCode` | fun | `hashCode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/LeaveRequestCodec.kt:27 Companion` | object | `Companion` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/LeaveRequestCodec.kt:28 create` | fun | `create` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/LeaveRequestCodec.kt:35 ContentTypeLeaveRequest` | val | `ContentTypeLeaveRequest` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/LeaveRequestCodec.kt:43 LeaveRequestCodec` | class | `LeaveRequestCodec` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/LeaveRequestCodec.kt:44 contentType` | constructor property | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/LeaveRequestCodec.kt:46 encode` | fun | `encode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/LeaveRequestCodec.kt:57 decode` | fun | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/LeaveRequestCodec.kt:65 fallback` | fun | `fallback` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/LeaveRequestCodec.kt:67 shouldPush` | fun | `shouldPush` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:14 ContentTypeMultiRemoteAttachment` | val | `ContentTypeMultiRemoteAttachment` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:22 MultiRemoteAttachment` | class | `MultiRemoteAttachment` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:23 remoteAttachments` | constructor property | `remoteAttachments` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:26 RemoteAttachmentInfo` | class | `RemoteAttachmentInfo` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:27 url` | constructor property | `url` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:28 filename` | constructor property | `filename` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:29 contentLength` | constructor property | `contentLength` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:30 contentDigest` | constructor property | `contentDigest` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:31 nonce` | constructor property | `nonce` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:32 scheme` | constructor property | `scheme` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:33 salt` | constructor property | `salt` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:34 secret` | constructor property | `secret` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:36 Companion` | object | `Companion` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:37 from` | fun | `from` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:59 MultiRemoteAttachmentCodec` | class | `MultiRemoteAttachmentCodec` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:60 contentType` | constructor property | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:62 encode` | fun | `encode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:82 decode` | fun | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:101 fallback` | fun | `fallback` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:103 shouldPush` | fun | `shouldPush` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:105 Companion` | object | `Companion` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:106 encryptBytesForLocalAttachment` | fun | `encryptBytesForLocalAttachment` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:111 buildRemoteAttachmentInfo` | fun | `buildRemoteAttachmentInfo` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:116 buildEncryptAttachmentResult` | fun | `buildEncryptAttachmentResult` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/MultiRemoteAttachmentCodec.kt:130 decryptAttachment` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:13 ContentTypeReaction` | val | `ContentTypeReaction` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:21 Reaction` | class | `Reaction` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:22 reference` | constructor property | `reference` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:23 action` | constructor property | `action` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:24 content` | constructor property | `content` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:25 schema` | constructor property | `schema` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:26 referenceInboxId` | constructor property | `referenceInboxID` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:29 ReactionAction` | class | `ReactionAction` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:30 Removed` | object | `Removed` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:32 Added` | object | `Added` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:34 Unknown` | object | `Unknown` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:37 ReactionSchema` | class | `ReactionSchema` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:38 Unicode` | object | `Unicode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:40 Shortcode` | object | `Shortcode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:42 Custom` | object | `Custom` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:44 Unknown` | object | `Unknown` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:47 getReactionSchema` | fun | `getReactionSchema` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:55 getReactionAction` | fun | `getReactionAction` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:62 ReactionCodec` | class | `ReactionCodec` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:63 contentType` | constructor property | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:65 encode` | fun | `encode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:79 decode` | fun | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:100 fallback` | fun | `fallback` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionCodec.kt:107 shouldPush` | fun | `shouldPush` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionV2Codec.kt:9 ContentTypeReactionV2` | val | `ContentTypeReactionV2` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionV2Codec.kt:65 ReactionV2Codec` | class | `ReactionV2Codec` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionV2Codec.kt:66 contentType` | constructor property | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionV2Codec.kt:68 encode` | fun | `encode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionV2Codec.kt:71 decode` | fun | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionV2Codec.kt:73 fallback` | fun | `fallback` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReactionV2Codec.kt:80 shouldPush` | fun | `shouldPush` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReadReceiptCodec.kt:5 ContentTypeReadReceipt` | val | `ContentTypeReadReceipt` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReadReceiptCodec.kt:13 ReadReceipt` | object | `ReadReceipt` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReadReceiptCodec.kt:15 ReadReceiptCodec` | class | `ReadReceiptCodec` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReadReceiptCodec.kt:16 contentType` | constructor property | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReadReceiptCodec.kt:18 encode` | fun | `encode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReadReceiptCodec.kt:26 decode` | fun | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReadReceiptCodec.kt:28 fallback` | fun | `fallback` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReadReceiptCodec.kt:30 shouldPush` | fun | `shouldPush` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:16 EncryptedEncodedContent` | class | `EncryptedEncodedContent` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:17 contentDigest` | constructor property | `contentDigest` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:18 secret` | constructor property | `secret` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:19 salt` | constructor property | `salt` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:20 nonce` | constructor property | `nonce` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:21 payload` | constructor property | `payload` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:22 contentLength` | constructor property | `contentLength` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:23 filename` | constructor property | `filename` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:26 RemoteAttachment` | class | `RemoteAttachment` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:27 url` | constructor property | `url` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:28 contentDigest` | constructor property | `contentDigest` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:29 secret` | constructor property | `secret` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:30 salt` | constructor property | `salt` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:31 nonce` | constructor property | `nonce` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:32 scheme` | constructor property | `scheme` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:33 contentLength` | constructor property | `contentLength` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:34 filename` | constructor property | `filename` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:35 fetcher` | constructor property | `fetcher` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:37 load` | fun | `RemoteAttachmentDownload` | platform helper | 2 | Native HTTPS download stays under sdks/ (2, 11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:60 Companion` | object | `Companion` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:61 decryptEncoded` | fun | `decryptEncoded` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:87 encodeEncrypted` | fun | `encodeEncrypted` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:109 encodeEncryptedBytes` | fun | `encodeEncryptedBytes` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:130 from` | fun | `from` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:150 ContentTypeRemoteAttachment` | val | `ContentTypeRemoteAttachment` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:158 Fetcher` | interface | `Fetcher` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:159 fetch` | fun | `fetch` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:162 HTTPFetcher` | class | `HTTPFetcher` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:163 fetch` | fun | `fetch` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:166 RemoteAttachmentCodec` | class | `RemoteAttachmentCodec` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:167 contentType` | constructor property | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:169 encode` | fun | `encode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:188 decode` | fun | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:212 fallback` | fun | `fallback` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/RemoteAttachmentCodec.kt:215 shouldPush` | fun | `shouldPush` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReplyCodec.kt:6 ContentTypeReply` | val | `ContentTypeReply` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReplyCodec.kt:14 Reply` | class | `Reply` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReplyCodec.kt:15 reference` | constructor property | `reference` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReplyCodec.kt:16 content` | constructor property | `content` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReplyCodec.kt:17 contentType` | constructor property | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReplyCodec.kt:20 ReplyCodec` | class | `ReplyCodec` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReplyCodec.kt:21 contentType` | constructor property | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReplyCodec.kt:23 encode` | fun | `encode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReplyCodec.kt:37 decode` | fun | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReplyCodec.kt:52 fallback` | fun | `fallback` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/ReplyCodec.kt:54 shouldPush` | fun | `shouldPush` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TextCodec.kt:6 ContentTypeText` | val | `ContentTypeText` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TextCodec.kt:14 TextCodec` | class | `TextCodec` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TextCodec.kt:15 contentType` | constructor property | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TextCodec.kt:17 encode` | fun | `encode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TextCodec.kt:26 decode` | fun | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TextCodec.kt:39 fallback` | fun | `fallback` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TextCodec.kt:41 shouldPush` | fun | `shouldPush` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:3 ContentTypeTransactionReference` | val | `ContentTypeTransactionReference` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:11 TransactionReference` | class | `TransactionReference` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:12 namespace` | constructor property | `namespace` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:13 networkId` | constructor property | `networkID` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:14 reference` | constructor property | `reference` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:15 metadata` | constructor property | `kind / creatorInboxID` | alias | 11.4 Kotlin; 19.2 | Deprecated name for one major release (19.2; 11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:17 Metadata` | class | `Metadata` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:18 transactionType` | constructor property | `transactionType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:19 currency` | constructor property | `currency` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:20 amount` | constructor property | `amount` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:21 decimals` | constructor property | `decimals` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:22 fromAddress` | constructor property | `fromAddress` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:23 toAddress` | constructor property | `toAddress` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:27 TransactionReferenceCodec` | class | `TransactionReferenceCodec` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:28 contentType` | constructor property | `contentType` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:30 encode` | fun | `encode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:54 decode` | fun | `decode` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:75 fallback` | fun | `fallback` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/TransactionReferenceCodec.kt:78 shouldPush` | fun | `shouldPush` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/codecs/WalletSendCalls.kt:3 WalletSendCalls` | typealias | `WalletSendCalls` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:7 ArchiveOptions` | class | `ArchiveOptions` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:8 startNs` | constructor property | `startNs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:9 endNs` | constructor property | `endNs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:10 archiveElements` | constructor property | `archiveElements` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:11 excludeDisappearingMessages` | val | `excludeDisappearingMessages` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:14 ArchiveOptions` | fun | `ArchiveOptions` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:22 ArchiveElement` | class | `ArchiveElement` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:23 MESSAGES` | enum case | `MESSAGES` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:24 CONSENT` | enum case | `CONSENT` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:27 toFfi` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:28 when` | enum case | `when` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:33 Companion` | object | `Companion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:34 fromFfi` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:42 ArchiveMetadata` | class | `ArchiveMetadata` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:45 archiveVersion` | val | `archiveVersion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:46 elements` | val | `elements` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:53 exportedAtNs` | val | `exportedAt.ns` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:54 startNs` | val | `startNs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ArchiveOptions.kt:55 endNs` | val | `endNs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ConversationDebugInfo.kt:5 ConversationDebugInfo` | class | `ConversationDebugInfo` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ConversationDebugInfo.kt:8 CommitLogForkStatus` | class | `CommitLogForkStatus` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ConversationDebugInfo.kt:9 FORKED` | enum case | `FORKED` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ConversationDebugInfo.kt:10 NOT_FORKED` | enum case | `NOT_FORKED` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ConversationDebugInfo.kt:11 UNKNOWN` | enum case | `UNKNOWN` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ConversationDebugInfo.kt:14 epoch` | val | `epoch` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ConversationDebugInfo.kt:16 maybeForked` | val | `maybeForked` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ConversationDebugInfo.kt:18 forkDetails` | val | `forkDetails` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ConversationDebugInfo.kt:20 localCommitLog` | val | `localCommitLog` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ConversationDebugInfo.kt:22 remoteCommitLog` | val | `remoteCommitLog` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/ConversationDebugInfo.kt:24 commitLogForkStatus` | val | `commitLogForkStatus` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/DecodedMessage.kt:18 encodedContent` | constructor property | `encodedContent` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/DecodedMessage.kt:21 deliveryCursor` | constructor property | `deliveryCursor` | static runtime | 11.4 Kotlin | Host message, codec, preference, or stream runtime (2, 11.7). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/DisappearingMessageSettings.kt:5 DisappearingMessageSettings` | class | `DisappearingMessageSettings` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/DisappearingMessageSettings.kt:6 disappearStartingAtNs` | constructor property | `disappearStartingAt.ns` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/DisappearingMessageSettings.kt:7 retentionDurationInNs` | constructor property | `retentionDurationInNs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/DisappearingMessageSettings.kt:9 Companion` | object | `Companion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/DisappearingMessageSettings.kt:10 createFromFfi` | fun | `createFromFfi` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:17 MlsExtensionType` | class | `MlsExtensionType` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:18 ApplicationId` | object | `ApplicationID` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:20 RatchetTree` | object | `RatchetTree` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:22 RequiredCapabilities` | object | `RequiredCapabilities` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:24 ExternalPub` | object | `ExternalPub` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:26 ExternalSenders` | object | `ExternalSenders` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:28 LastResort` | object | `LastResort` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:30 ImmutableMetadata` | object | `ImmutableMetadata` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:32 AppDataDictionary` | object | `AppDataDictionary` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:34 Unknown` | class | `Unknown` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:35 id` | constructor property | `id` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:38 Grease` | class | `Grease` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:39 id` | constructor property | `id` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:62 InstallationCapabilities` | class | `InstallationCapabilities` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:66 installationId` | val | `installationID` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:70 isOwn` | val | `isOwn` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:77 supportedExtensions` | val | `supportedExtensions` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:85 capabilitiesKnown` | val | `capabilitiesKnown` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:93 InboxCapabilities` | class | `InboxCapabilities` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:96 inboxId` | val | `inboxID` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:99 installations` | val | `installations` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:118 GroupMembershipCapabilities` | class | `GroupMembershipCapabilities` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:122 contextExtensions` | val | `contextExtensions` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipCapabilities.kt:126 members` | val | `members` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipResult.kt:7 GroupMembershipResult` | class | `GroupMembershipResult` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipResult.kt:10 addedMembers` | val | `addedMembers` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipResult.kt:12 removedMembers` | val | `removedMembers` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipResult.kt:14 failedInstallationIds` | val | `failedInstallationIDs` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipState.kt:10 GroupMembershipState` | class | `GroupMembershipState` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipState.kt:14 ALLOWED` | enum case | `ALLOWED` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipState.kt:19 REJECTED` | enum case | `REJECTED` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipState.kt:24 PENDING` | enum case | `PENDING` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipState.kt:29 RESTORED` | enum case | `RESTORED` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipState.kt:34 PENDING_REMOVE` | enum case | `PENDING_REMOVE` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipState.kt:38 Companion` | object | `Companion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipState.kt:42 fromFfiGroupMembershipState` | fun | `fromFfiGroupMembershipState` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipState.kt:55 toFfi` | fun | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/GroupMembershipState.kt:56 when` | enum case | `when` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/InboxState.kt:7 SignatureKind` | typealias | `SignatureKind` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/InboxState.kt:9 InboxState` | class | `InboxState` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/InboxState.kt:12 inboxId` | val | `inboxID` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/InboxState.kt:14 identities` | val | `identities` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/InboxState.kt:17 installations` | val | `installations` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/InboxState.kt:20 recoveryPublicIdentity` | val | `recoveryPublicIdentity` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/InboxState.kt:23 creationSignatureKind` | val | `creationSignatureKind` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Installation.kt:7 Installation` | class | `Installation` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Installation.kt:10 installationId` | val | `installationID` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Installation.kt:12 createdAt` | val | `createdAt` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Member.kt:8 PermissionLevel` | class | `PermissionLevel` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Member.kt:9 MEMBER` | enum case | `MEMBER` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Member.kt:10 ADMIN` | enum case | `ADMIN` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Member.kt:11 SUPER_ADMIN` | enum case | `SUPER_ADMIN` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Member.kt:14 Member` | class | `Member` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Member.kt:17 inboxId` | val | `inboxID` | alias | 11.4 Kotlin; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Member.kt:19 identities` | val | `identities` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Member.kt:21 permissionLevel` | val | `permissionLevel` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Member.kt:29 consentState` | val | `consentState` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:7 PermissionOption` | class | `PermissionOption` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:8 Allow` | enum case | `Allow` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:9 Deny` | enum case | `Deny` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:10 Admin` | enum case | `Admin` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:11 SuperAdmin` | enum case | `SuperAdmin` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:12 Unknown` | enum case | `Unknown` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:15 Companion` | object | `Companion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:16 toFfiPermissionPolicy` | fun | `toFfiPermissionPolicy` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:25 fromFfiPermissionPolicy` | fun | `fromFfiPermissionPolicy` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:37 GroupPermissionPreconfiguration` | class | `GroupPermissionPreconfiguration` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:38 ALL_MEMBERS` | enum case | `ALL_MEMBERS` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:39 ADMIN_ONLY` | enum case | `ADMIN_ONLY` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:42 Companion` | object | `Companion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:43 toFfiGroupPermissionOptions` | fun | `toFfiGroupPermissionOptions` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:51 PermissionPolicySet` | class | `PermissionPolicySet` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:52 addMemberPolicy` | constructor property | `addMemberPolicy` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:53 removeMemberPolicy` | constructor property | `removeMemberPolicy` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:54 addAdminPolicy` | constructor property | `addAdminPolicy` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:55 removeAdminPolicy` | constructor property | `removeAdminPolicy` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:56 updateGroupNamePolicy` | constructor property | `updateGroupNamePolicy` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:57 updateGroupDescriptionPolicy` | constructor property | `updateGroupDescriptionPolicy` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:58 updateGroupImagePolicy` | constructor property | `updateGroupImagePolicy` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:59 updateMessageDisappearingPolicy` | constructor property | `updateMessageDisappearingPolicy` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:60 updateAppDataPolicy` | constructor property | `updateAppDataPolicy` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:62 Companion` | object | `Companion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:63 toFfiPermissionPolicySet` | fun | `toFfiPermissionPolicySet` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PermissionPolicySet.kt:91 fromFfiPermissionPolicySet` | fun | `fromFfiPermissionPolicySet` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PublicIdentity.kt:6 IdentityKind` | class | `IdentityKind` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PublicIdentity.kt:7 ETHEREUM` | enum case | `ETHEREUM` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PublicIdentity.kt:8 PASSKEY` | enum case | `PASSKEY` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PublicIdentity.kt:11 PublicIdentity` | class | `PublicIdentity` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PublicIdentity.kt:12 ffiPrivate` | constructor property | — | approved removal | 11.4 Kotlin | Old binding helper leaves the public API (11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PublicIdentity.kt:14 constructor` | constructor | `constructor` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PublicIdentity.kt:26 kind` | val | `kind` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PublicIdentity.kt:29 identifier` | val | `identifier` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PublicIdentity.kt:33 IdentityKind` | fun | `IdentityKind` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/PublicIdentity.kt:39 FfiIdentifierKind` | fun | `FfiIdentifierKind` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Reply.kt:5 Reply` | class | `Reply` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Reply.kt:6 inReplyTo` | constructor property | `inReplyTo` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Reply.kt:7 content` | constructor property | `content` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Reply.kt:8 referenceId` | constructor property | `referenceID` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Reply.kt:10 Companion` | object | `Companion` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/Reply.kt:11 create` | fun | `create` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/SignatureRequest.kt:5 SignatureRequest` | class | `SignatureRequest` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/SignatureRequest.kt:6 ffiSignatureRequest` | constructor property | `unsafeCreateInboxSignatureRequest()` | alias | 11.4 Kotlin; 19.2 | Delicate flow takes the canonical unsafe name (11.4, 19.24). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/SignatureRequest.kt:8 addScwSignature` | fun | `addScwSignature` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/SignatureRequest.kt:17 addEcdsaSignature` | fun | `addEcdsaSignature` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/libxmtp/SignatureRequest.kt:21 signatureText` | fun | `signatureText` | generated | 11.4 Kotlin | Facade schema or generated record (11.1-11.4). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/messages/PrivateKey.kt:17 PrivateKey` | typealias | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/messages/PrivateKey.kt:18 PublicKey` | typealias | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/messages/PrivateKey.kt:20 PrivateKeyBuilder` | class | — | approved removal | 11.4 Kotlin | Removal or replacement approved in 11.4 and 19. |
| `sdks/android/library/src/main/java/org/xmtp/android/library/messages/PrivateKey.kt:23 constructor` | constructor | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/messages/PrivateKey.kt:27 constructor` | constructor | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/messages/PrivateKey.kt:31 Companion` | object | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/messages/PrivateKey.kt:32 buildFromPrivateKeyData` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/messages/PrivateKey.kt:62 getPrivateKey` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/messages/PrivateKey.kt:64 publicIdentity` | val | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/messages/PrivateKey.kt:67 sign` | fun | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/messages/PrivateKey.kt:85 PrivateKey` | val | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/messages/PrivateKey.kt:88 PublicKey` | val | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |
| `sdks/android/library/src/main/java/org/xmtp/android/library/messages/PrivateKey.kt:90 address` | val | — | approved removal | 11.4 Kotlin | Replaced by Rust signer or encryption (11.4, 19.32/34). |

## Node

| Current export | Kind | Final name | Status | Design ref | Notes |
| --- | --- | --- | --- | --- | --- |
| `@xmtp/node-bindings:29 DeliveryCursor` | binding re-export | `DeliveryCursor` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:29 MessageCatchUp` | binding re-export | `MessageCatchUp` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:29 MessageCatchUpGeneration` | binding re-export | `MessageCatchUpGeneration` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:29 MessageHistorySnapshot` | binding re-export | `MessageHistorySnapshot` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:29 MessageTopicStatus` | binding re-export | `MessageTopicStatus` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Action` | binding re-export | `Action` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Actions` | binding re-export | `Actions` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 ApiStats` | binding re-export | `ApiStats` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 AppDataChange` | binding re-export | `AppDataChange` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 ArchiveMetadata` | binding re-export | `ArchiveMetadata` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 ArchiveOptions` | binding re-export | `ArchiveOptions` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Attachment` | binding re-export | `Attachment` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 AuthConfiguration` | binding re-export | `AuthConfiguration` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Backend` | binding re-export | `Backend` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 BackendBuilder` | binding re-export | `BackendBuilder` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Consent` | binding re-export | `Consent` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 ConversationDebugInfo` | binding re-export | `ConversationDebugInfo` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 ConversationListItem` | binding re-export | `ConversationListItem` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 CreateDmOptions` | binding re-export | `CreateDmOptions` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 CreateGroupOptions` | binding re-export | `CreateGroupOptions` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Cursor` | binding re-export | `Cursor` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 EncryptedAttachment` | binding re-export | `EncryptedAttachment` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 GroupMember` | binding re-export | `GroupMember` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 GroupMetadata` | binding re-export | `GroupMetadata` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 GroupPermissions` | binding re-export | `GroupPermissions` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 GroupSyncSummary` | binding re-export | `GroupSyncSummary` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 GroupUpdated` | binding re-export | `GroupUpdated` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 HmacKey` | binding re-export | `HmacKey` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Identifier` | binding re-export | `Identifier` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 IdentityStats` | binding re-export | `IdentityStats` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Inbox` | binding re-export | `Inbox` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 InboxState` | binding re-export | `InboxState` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Installation` | binding re-export | `Installation` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Intent` | binding re-export | `Intent` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 KeyPackageStatus` | binding re-export | `KeyPackageStatus` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 LeaveRequest` | binding re-export | `LeaveRequest` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Lifetime` | binding re-export | `Lifetime` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 LimitsConfiguration` | binding re-export | `LimitsConfiguration` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 ListConversationsOptions` | binding re-export | `ListConversationsOptions` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 ListMessagesOptions` | binding re-export | `ListMessagesOptions` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 LogOptions` | binding re-export | `LogOptions` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Message` | binding re-export | `Message` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 MessageDisappearingSettings` | binding re-export | `MessageDisappearingSettings` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 MetadataFieldChange` | binding re-export | `MetadataFieldChange` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 MlsConfiguration` | binding re-export | `MlsConfiguration` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 MultiRemoteAttachment` | binding re-export | `MultiRemoteAttachment` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 PermissionPolicySet` | binding re-export | `PermissionPolicySet` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Reaction` | binding re-export | `Reaction` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 ReadReceipt` | binding re-export | `ReadReceipt` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 RemoteAttachment` | binding re-export | `RemoteAttachment` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 Reply` | binding re-export | `Reply` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 RetentionConfiguration` | binding re-export | `RetentionConfiguration` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 SendMessageOpts` | binding re-export | `SendMessageOpts` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 SendOpts` | binding re-export | `SendOpts` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 ServerConfiguration` | binding re-export | `ServerConfiguration` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 SignatureRequestHandle` | binding re-export | `SignatureRequestHandle` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 SigningKeyDescription` | binding re-export | `SigningKeyDescription` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 TransactionMetadata` | binding re-export | `TransactionMetadata` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 TransactionReference` | binding re-export | `TransactionReference` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 UserPreferenceUpdate` | binding re-export | `UserPreferenceUpdate` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 VisibilityConfirmationOptions` | binding re-export | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `@xmtp/node-bindings:51 WalletCall` | binding re-export | `WalletCall` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 WalletSendCalls` | binding re-export | `WalletSendCalls` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 WorkerConfigOptions` | binding re-export | `WorkerConfigOptions` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 WorkerIntervalOverride` | binding re-export | `WorkerIntervalOverride` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:51 WorkerJitterOverride` | binding re-export | `WorkerJitterOverride` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 ActionStyle` | binding re-export | `ActionStyle` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 BackupElementSelectionOption` | binding re-export | `BackupElementSelectionOption` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 ConsentEntityType` | binding re-export | `ConsentEntityType` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 ConsentState` | binding re-export | `ConsentState` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 ContentType` | binding re-export | `ContentType` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 ConversationType` | binding re-export | `ConversationType` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 DeliveryStatus` | binding re-export | `DeliveryStatus` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 GroupMembershipState` | binding re-export | `GroupMembershipState` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 GroupMessageKind` | binding re-export | `GroupMessageKind` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 GroupPermissionsOptions` | binding re-export | `GroupPermissionsOptions` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 IdentifierKind` | binding re-export | `IdentifierKind` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 ListConversationsOrderBy` | binding re-export | `ListConversationsOrderBy` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 LogLevel` | binding re-export | `LogLevel` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 MessageSortBy` | binding re-export | `MessageSortBy` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 MetadataField` | binding re-export | `MetadataField` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 PermissionLevel` | binding re-export | `PermissionLevel` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 PermissionPolicy` | binding re-export | `PermissionPolicy` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 PermissionUpdateType` | binding re-export | `PermissionUpdateType` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 ReactionAction` | binding re-export | `ReactionAction` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 ReactionSchema` | binding re-export | `ReactionSchema` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 SortDirection` | binding re-export | `SortDirection` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 WorkerKind` | binding re-export | `WorkerKind` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeActions` | binding re-export | `contentTypeActions` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeAttachment` | binding re-export | `contentTypeAttachment` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeGroupUpdated` | binding re-export | `contentTypeGroupUpdated` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeIntent` | binding re-export | `contentTypeIntent` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeLeaveRequest` | binding re-export | `contentTypeLeaveRequest` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeMarkdown` | binding re-export | `contentTypeMarkdown` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeMultiRemoteAttachment` | binding re-export | `contentTypeMultiRemoteAttachment` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeReaction` | binding re-export | `contentTypeReaction` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeReadReceipt` | binding re-export | `contentTypeReadReceipt` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeRemoteAttachment` | binding re-export | `contentTypeRemoteAttachment` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeReply` | binding re-export | `contentTypeReply` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeText` | binding re-export | `contentTypeText` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeTransactionReference` | binding re-export | `contentTypeTransactionReference` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 contentTypeWalletSendCalls` | binding re-export | `contentTypeWalletSendCalls` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 decryptAttachment` | binding re-export | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `@xmtp/node-bindings:114 encodeActions` | binding re-export | `encodeActions` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 encodeAttachment` | binding re-export | `encodeAttachment` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 encodeIntent` | binding re-export | `encodeIntent` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 encodeMarkdown` | binding re-export | `encodeMarkdown` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 encodeMultiRemoteAttachment` | binding re-export | `encodeMultiRemoteAttachment` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 encodeReaction` | binding re-export | `encodeReaction` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 encodeReadReceipt` | binding re-export | `encodeReadReceipt` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 encodeRemoteAttachment` | binding re-export | `encodeRemoteAttachment` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 encodeText` | binding re-export | `encodeText` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 encodeTransactionReference` | binding re-export | `encodeTransactionReference` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 encodeWalletSendCalls` | binding re-export | `encodeWalletSendCalls` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 encryptAttachment` | binding re-export | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `@xmtp/node-bindings:114 flushTelemetry` | binding re-export | `flushTelemetry` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `@xmtp/node-bindings:114 initLogging` | binding re-export | `initLogging` | generated | 11.4 Node | Binding export supplied by the facade generator (11.4). |
| `sdks/node/src/Client.ts:112 Client.constructor` | member | `Client.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:125 Client.init` | member | `Client.init` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:155 Client.create` | member | `Client.create` | static runtime | 11.4 Node | Host wrapper owns codecs and the client registry (11.1, 11.7). |
| `sdks/node/src/Client.ts:156 Client.signer` | member | `Client.signer` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:157 Client.options` | member | `Client.options` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:183 Client.build` | member | `Client.build` | static runtime | 11.4 Node | Host wrapper owns codecs and the client registry (11.1, 11.7). |
| `sdks/node/src/Client.ts:184 Client.identifier` | member | `Client.identifier` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:200 Client.libxmtpVersion` | member | `Client.libxmtpVersion` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:207 Client.appVersion` | member | `Client.appVersion` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:216 Client.env` | member | `storage.label` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Client.ts:240 Client.accountIdentifier` | member | `Client.accountIdentifier` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:247 Client.inboxId` | member | `inboxID` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Client.ts:257 Client.installationId` | member | `installationID` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Client.ts:267 Client.installationIdBytes` | member | `Client.installationIdBytes` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:279 Client.isRegistered` | member | `Client.isRegistered` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:291 Client.conversations` | member | `Client.conversations` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:303 Client.debugInformation` | member | `diagnostics` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Client.ts:315 Client.preferences` | member | `Client.preferences` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:334 Client.close` | member | `end()` | alias | 11.4 Node; 19.2 | Async client and reader shutdown uses end() (plan Decisions, Section 20.8). |
| `sdks/node/src/Client.ts:357 Client.unsafe_addSignature` | member | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/Client.ts:358 Client.signatureRequest` | member | `Client.signatureRequest` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:406 Client.unsafe_createInboxSignatureRequest` | member | `Client.unsafe_createInboxSignatureRequest` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:432 Client.unsafe_addAccountSignatureRequest` | member | `Client.unsafe_addAccountSignatureRequest` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:433 Client.newAccountIdentifier` | member | `Client.newAccountIdentifier` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:434 Client.allowInboxReassign` | member | `Client.allowInboxReassign` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:461 Client.unsafe_removeAccountSignatureRequest` | member | `Client.unsafe_removeAccountSignatureRequest` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:482 Client.unsafe_revokeAllOtherInstallationsSignatureRequest` | member | `Client.unsafe_revokeAllOtherInstallationsSignatureRequest` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:504 Client.unsafe_revokeInstallationsSignatureRequest` | member | `Client.unsafe_revokeInstallationsSignatureRequest` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:505 Client.installationIds` | member | `Client.installationIDs` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:528 Client.unsafe_changeRecoveryIdentifierSignatureRequest` | member | `Client.unsafe_changeRecoveryIdentifierSignatureRequest` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:551 Client.unsafe_applySignatureRequest` | member | `Client.unsafe_applySignatureRequest` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:567 Client.register` | member | `Client.register` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:598 Client.unsafe_addAccount` | member | `Client.unsafe_addAccount` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:599 Client.newAccountSigner` | member | `Client.newAccountSigner` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:628 Client.removeAccount` | member | `Client.removeAccount` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:644 Client.revokeAllOtherInstallations` | member | `Client.revokeAllOtherInstallations` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:666 Client.revokeInstallations` | member | `Client.revokeInstallations` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:679 Client.optionsOrBackend` | member | `Client.optionsOrBackend` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:722 Client.changeRecoveryIdentifier` | member | `Client.changeRecoveryIdentifier` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:737 Client.canMessage` | member | `Client.canMessage` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:753 Client.fetchLatestInboxUpdatesCount` | member | `Client.fetchLatestInboxUpdatesCount` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:769 Client.fetchOwnInboxUpdatesCount` | member | `Client.fetchOwnInboxUpdatesCount` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:785 Client.fetchKeyPackageStatuses` | member | `Client.fetchKeyPackageStatuses` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:803 Client.fetchInboxIdByIdentifier` | member | `Client.fetchInboxIdByIdentifier` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:818 Client.signWithInstallationKey` | member | `Client.signWithInstallationKey` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:834 Client.verifySignedWithInstallationKey` | member | `Client.verifySignedWithInstallationKey` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:835 Client.signatureText` | member | `Client.signatureText` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:836 Client.signatureBytes` | member | `Client.signatureBytes` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:854 Client.fetchInboxStates` | member | `Client.fetchInboxStates` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:855 Client.inboxIds` | member | `Client.inboxIDs` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:885 Client.identifiers` | member | `Client.identifiers` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:905 Client.verifySignedWithPublicKey` | member | `Client.verifySignedWithPublicKey` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:908 Client.publicKey` | member | `Client.publicKey` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:923 Client.isAddressAuthorized` | member | `Client.isAddressAuthorized` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:925 Client.address` | member | `Client.address` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:933 Client.isInstallationAuthorized` | member | `Client.isInstallationAuthorized` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:935 Client.installation` | member | `Client.installation` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:967 Client.createArchive` | member | `archives.exportToFile()` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Client.ts:984 Client.importArchive` | member | `archives.importFromFile()` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Client.ts:1001 Client.archiveMetadata` | member | `archives.metadataFromFile()` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Client.ts:1002 Client.path` | member | `Client.path` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:1003 Client.key` | member | `Client.key` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:1017 Client.syncAllDeviceSyncGroups` | member | `Client.syncAllDeviceSyncGroups` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:1026 Client.enableNotifications` | member | `Client.enableNotifications` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:1027 Client.config` | member | `Client.config` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:1042 Client.disableNotifications` | member | `Client.disableNotifications` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:1052 Client.notificationState` | member | `Client.notificationState` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:1069 Client.serverConfiguration` | member | `Client.serverConfiguration` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:1090 Client.refreshServerConfiguration` | member | `Client.refreshServerConfiguration` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:1112 Client.fetchServerConfiguration` | member | `Client.fetchServerConfiguration` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Client.ts:1113 Client.target` | member | `Client.target` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/CodecRegistry.ts:10 CodecRegistry.constructor` | member | `CodecRegistry.constructor` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/CodecRegistry.ts:22 CodecRegistry.getCodec` | member | `CodecRegistry.getCodec` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/Conversation.ts:53 Conversation.constructor` | member | `Conversation.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:54 Conversation._client` | member | `Conversation._client` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:55 Conversation.codecRegistry` | member | `Conversation.codecRegistry` | static runtime | 11.4 Node | Per-client codec registry stays in the host runtime (11.4). |
| `sdks/node/src/Conversation.ts:56 Conversation.conversation` | member | `Conversation.conversation` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:65 Conversation.id` | member | `Conversation.id` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:72 Conversation.isActive` | member | `state().isActive` | generated | 11.4 Node | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/node/src/Conversation.ts:77 Conversation.setNotifications` | member | `Conversation.setNotifications` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:84 Conversation.notificationsEnabled` | member | `state().notificationsEnabled` | generated | 11.4 Node | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/node/src/Conversation.ts:93 Conversation.addedByInboxId` | member | `Conversation.addedByInboxID` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:100 Conversation.createdAtNs` | member | `Conversation.createdAt.ns` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:107 Conversation.createdAt` | member | `Conversation.createdAt` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:111 Conversation.topic` | member | `Conversation.topic` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:115 Conversation.pausedForVersion` | member | `state().pausedForVersion` | generated | 11.4 Node | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/node/src/Conversation.ts:124 Conversation.hmacKeys` | member | `Conversation.hmacKeys` | generated | 11.4 Node | Returns ID-keyed entry records, not a map (plan Decisions, Section 20.7). |
| `sdks/node/src/Conversation.ts:133 Conversation.metadata` | member | — | approved removal | 11.4 Node | Immutable kind and creatorInboxID replace metadata() (11.4). |
| `sdks/node/src/Conversation.ts:146 Conversation.members` | member | `Conversation.members` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:155 Conversation.sync` | member | `Conversation.sync` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:168 Conversation.stream` | member | `Conversation.stream` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:169 Conversation.options` | member | `Conversation.options` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:193 Conversation.messageHistorySnapshot` | member | `Conversation.messageHistorySnapshot` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:197 Conversation.beginningDeliveryCursor` | member | `Conversation.beginningDeliveryCursor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:207 Conversation.processStreamedMessage` | member | `Conversation.processStreamedMessage` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:216 Conversation.publishMessages` | member | `Conversation.publishMessages` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:233 Conversation.send` | member | `Conversation.send` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:250 Conversation.sendText` | member | `Conversation.sendText` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:261 Conversation.sendMarkdown` | member | `Conversation.sendMarkdown` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:272 Conversation.sendReaction` | member | `Conversation.sendReaction` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:282 Conversation.sendReadReceipt` | member | `Conversation.sendReadReceipt` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:293 Conversation.sendReply` | member | `Conversation.sendReply` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:304 Conversation.sendTransactionReference` | member | `Conversation.sendTransactionReference` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:305 Conversation.transactionReference` | member | `Conversation.transactionReference` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:306 Conversation.opts` | member | `Conversation.opts` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:321 Conversation.sendWalletSendCalls` | member | `Conversation.sendWalletSendCalls` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:332 Conversation.sendActions` | member | `Conversation.sendActions` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:343 Conversation.sendIntent` | member | `Conversation.sendIntent` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:354 Conversation.sendAttachment` | member | `Conversation.sendAttachment` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:365 Conversation.sendMultiRemoteAttachment` | member | `Conversation.sendMultiRemoteAttachment` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:366 Conversation.multiRemoteAttachment` | member | `Conversation.multiRemoteAttachment` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:382 Conversation.sendRemoteAttachment` | member | `Conversation.sendRemoteAttachment` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:383 Conversation.remoteAttachment` | member | `Conversation.remoteAttachment` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:395 Conversation.messages` | member | `Conversation.messages` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:409 Conversation.countMessages` | member | `Conversation.countMessages` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:421 Conversation.lastMessage` | member | `Conversation.lastMessage` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:435 Conversation.consentState` | member | `state().consentState` | generated | 11.4 Node | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/node/src/Conversation.ts:444 Conversation.updateConsentState` | member | `Conversation.updateConsentState` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:453 Conversation.messageDisappearingSettings` | member | `state().disappearingSettings` | generated | 11.4 Node | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/node/src/Conversation.ts:464 Conversation.updateMessageDisappearingSettings` | member | `updateDisappearingSettings()` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Conversation.ts:476 Conversation.removeMessageDisappearingSettings` | member | `updateDisappearingSettings(null)` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Conversation.ts:485 Conversation.isMessageDisappearingEnabled` | member | `state().isDisappearingEnabled` | generated | 11.4 Node | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/node/src/Conversation.ts:494 Conversation.debugInfo` | member | `Conversation.debugInfo` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversation.ts:504 Conversation.lastReadTimes` | member | `Conversation.lastReadTimes` | generated | 11.4 Node | Returns ID-keyed entry records, not a map (plan Decisions, Section 20.7). |
| `sdks/node/src/Conversations.ts:48 Conversations.constructor` | member | `Conversations.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:49 Conversations.client` | member | `Conversations.client` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:50 Conversations.codecRegistry` | member | `Conversations.codecRegistry` | static runtime | 11.4 Node | Per-client codec registry stays in the host runtime (11.4). |
| `sdks/node/src/Conversations.ts:51 Conversations.conversations` | member | `Conversations.conversations` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:58 Conversations.topic` | member | `Conversations.topic` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:69 Conversations.getConversationById` | member | `getByID()` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Conversations.ts:98 Conversations.getDmByInboxId` | member | `Conversations.getDmByInboxID` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:115 Conversations.fetchDmByIdentifier` | member | `getDmByIdentity()` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Conversations.ts:130 Conversations.getMessageById` | member | `Conversations.getMessageByID` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:147 Conversations.createGroupOptimistic` | member | `Conversations.createGroupOptimistic` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:160 Conversations.createGroupWithIdentifiers` | member | `createGroupWithIdentities()` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Conversations.ts:161 Conversations.identifiers` | member | `Conversations.identifiers` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:162 Conversations.options` | member | `Conversations.options` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:184 Conversations.createGroup` | member | `Conversations.createGroup` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:202 Conversations.createDmWithIdentifier` | member | `createDmWithIdentity()` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Conversations.ts:203 Conversations.identifier` | member | `Conversations.identifier` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:226 Conversations.createDm` | member | `Conversations.createDm` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:243 Conversations.list` | member | `Conversations.list` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:277 Conversations.listGroups` | member | `Conversations.listGroups` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:299 Conversations.listDms` | member | `Conversations.listDms` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:320 Conversations.sync` | member | `Conversations.sync` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:332 Conversations.syncAll` | member | `Conversations.syncAll` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:344 Conversations.stream` | member | `Conversations.stream` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:347 Conversations.Group` | member | `Conversations.Group` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:390 Conversations.streamGroups` | member | `Conversations.streamGroups` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:417 Conversations.streamDms` | member | `Conversations.streamDms` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:443 Conversations.streamAllMessages` | member | `Conversations.streamAllMessages` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:476 Conversations.messageHistorySnapshot` | member | `Conversations.messageHistorySnapshot` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:477 Conversations.limit` | member | `Conversations.limit` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:492 Conversations.beginningDeliveryCursor` | member | `Conversations.beginningDeliveryCursor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:504 Conversations.streamAllGroupMessages` | member | `Conversations.streamAllGroupMessages` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:526 Conversations.streamAllDmMessages` | member | `Conversations.streamAllDmMessages` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:551 Conversations.streamMessageDeletions` | member | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/Conversations.ts:553 Conversations.StreamOptions` | member | `Conversations.StreamOptions` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Conversations.ts:578 Conversations.streamDeletedMessages` | member | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/Conversations.ts:604 Conversations.hmacKeys` | member | `Conversations.hmacKeys` | generated | 11.4 Node | Returns ID-keyed entry records, not a map (plan Decisions, Section 20.7). |
| `sdks/node/src/DebugInformation.ts:11 DebugInformation.constructor` | member | `DebugInformation.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/DebugInformation.ts:15 DebugInformation.apiStatistics` | member | `DebugInformation.apiStatistics` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/DebugInformation.ts:19 DebugInformation.apiIdentityStatistics` | member | `DebugInformation.apiIdentityStatistics` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/DebugInformation.ts:23 DebugInformation.apiAggregateStatistics` | member | `DebugInformation.apiAggregateStatistics` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/DebugInformation.ts:27 DebugInformation.clearAllStatistics` | member | `DebugInformation.clearAllStatistics` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/DecodedMessage.ts:177 DecodedMessage.deliveryCursor` | member | `DecodedMessage.deliveryCursor` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:178 DecodedMessage.content` | member | `DecodedMessage.content` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:179 DecodedMessage.contentType` | member | `DecodedMessage.contentType` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:180 DecodedMessage.conversationId` | member | `DecodedMessage.conversationID` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:181 DecodedMessage.deliveryStatus` | member | `DecodedMessage.deliveryStatus` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:182 DecodedMessage.expiresAtNs` | member | `DecodedMessage.expiresAt.ns` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:183 DecodedMessage.expiresAt` | member | `DecodedMessage.expiresAt` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:184 DecodedMessage.fallback` | member | `DecodedMessage.fallback` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:185 DecodedMessage.id` | member | `DecodedMessage.id` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:186 DecodedMessage.kind` | member | `DecodedMessage.kind` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:187 DecodedMessage.numReplies` | member | `DecodedMessage.numReplies` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:188 DecodedMessage.reactions` | member | `DecodedMessage.reactions` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:189 DecodedMessage.senderInboxId` | member | `DecodedMessage.senderInboxID` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:190 DecodedMessage.sentAt` | member | `DecodedMessage.sentAt` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:191 DecodedMessage.sentAtNs` | member | `DecodedMessage.sentAt.ns` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/DecodedMessage.ts:193 DecodedMessage.constructor` | member | `DecodedMessage.constructor` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/Dm.ts:24 Dm.constructor` | member | `Dm.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Dm.ts:25 Dm.client` | member | `Dm.client` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Dm.ts:26 Dm.codecRegistry` | member | `Dm.codecRegistry` | static runtime | 11.4 Node | Per-client codec registry stays in the host runtime (11.4). |
| `sdks/node/src/Dm.ts:27 Dm.conversation` | member | `Dm.conversation` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Dm.ts:40 Dm.peerInboxId` | member | `peerInboxID` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/Dm.ts:44 Dm.duplicateDms` | member | `Dm.duplicateDms` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:29 Group.constructor` | member | `Group.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:30 Group.client` | member | `Group.client` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:31 Group.codecRegistry` | member | `Group.codecRegistry` | static runtime | 11.4 Node | Per-client codec registry stays in the host runtime (11.4). |
| `sdks/node/src/Group.ts:32 Group.conversation` | member | `Group.conversation` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:41 Group.name` | member | `state().name` | generated | 11.4 Node | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/node/src/Group.ts:50 Group.updateName` | member | `Group.updateName` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:57 Group.imageUrl` | member | `state().imageUrl` | generated | 11.4 Node | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/node/src/Group.ts:66 Group.updateImageUrl` | member | `Group.updateImageUrl` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:73 Group.description` | member | `state().description` | generated | 11.4 Node | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/node/src/Group.ts:82 Group.updateDescription` | member | `Group.updateDescription` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:89 Group.appData` | member | `state().appData` | generated | 11.4 Node | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/node/src/Group.ts:105 Group.updateAppData` | member | `Group.updateAppData` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:115 Group.permissions` | member | `state().permissions` | generated | 11.4 Node | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/node/src/Group.ts:130 Group.updatePermission` | member | `Group.updatePermission` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:131 Group.permissionType` | member | `Group.permissionType` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:132 Group.policy` | member | `Group.policy` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:133 Group.metadataField` | member | `Group.metadataField` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:145 Group.listAdmins` | member | `Group.listAdmins` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:152 Group.listSuperAdmins` | member | `Group.listSuperAdmins` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:162 Group.isAdmin` | member | `Group.isAdmin` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:172 Group.isSuperAdmin` | member | `Group.isSuperAdmin` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:181 Group.addMembersByIdentifiers` | member | `Group.addMembersByIdentifiers` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:190 Group.addMembers` | member | `Group.addMembers` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:199 Group.removeMembersByIdentifiers` | member | `Group.removeMembersByIdentifiers` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:208 Group.removeMembers` | member | `Group.removeMembers` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:217 Group.addAdmin` | member | `Group.addAdmin` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:226 Group.removeAdmin` | member | `Group.removeAdmin` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:235 Group.addSuperAdmin` | member | `Group.addSuperAdmin` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:244 Group.removeSuperAdmin` | member | `Group.removeSuperAdmin` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:251 Group.requestRemoval` | member | `Group.requestRemoval` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Group.ts:260 Group.isPendingRemoval` | member | `Group.isPendingRemoval` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/MessageStream.ts:40 MessageStream.message` | member | `MessageStream.message` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:41 MessageStream.cursor` | member | `MessageStream.cursor` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:42 MessageStream.acknowledgement` | member | `MessageStream.acknowledgement` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:52 MessageStream.constructor` | member | `MessageStream.constructor` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:53 MessageStream.reader` | member | `MessageStream.reader` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:54 MessageStream.convert` | member | `MessageStream.convert` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:59 MessageStream.options` | member | `MessageStream.options` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:71 MessageStream.isDone` | member | `MessageStream.isDone` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:74 MessageStream.deliveredCursor` | member | `MessageStream.deliveredCursor` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:82 MessageStream.next` | member | `MessageStream.next` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:184 MessageStream.pending` | member | `MessageStream.pending` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:223 MessageStream.end` | member | `MessageStream.end` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:224 MessageStream.updateScope` | member | `MessageStream.updateScope` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:225 MessageStream.updateFilter` | member | `MessageStream.updateFilter` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:226 MessageStream.conversationType` | member | `MessageStream.conversationType` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:227 MessageStream.consentStates` | member | `MessageStream.consentStates` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:229 MessageStream.catchUpSnapshot` | member | `MessageStream.catchUpSnapshot` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/MessageStream.ts:230 MessageStream.catchUpChanged` | member | `MessageStream.catchUpChanged` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/Notifications.ts:34 NotificationError.constructor` | member | `NotificationError.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Notifications.ts:35 NotificationError.code` | member | `NotificationError.code` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Notifications.ts:36 NotificationError.options` | member | `NotificationError.options` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Preferences.ts:30 Preferences.constructor` | member | `Preferences.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Preferences.ts:35 Preferences.sync` | member | `Preferences.sync` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Preferences.ts:44 Preferences.inboxState` | member | `Preferences.inboxState` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Preferences.ts:53 Preferences.fetchInboxState` | member | `Preferences.fetchInboxState` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Preferences.ts:64 Preferences.getInboxStates` | member | `Preferences.getInboxStates` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Preferences.ts:74 Preferences.fetchInboxStates` | member | `Preferences.fetchInboxStates` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Preferences.ts:84 Preferences.setConsentStates` | member | `Preferences.setConsentStates` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Preferences.ts:95 Preferences.getConsentState` | member | `Preferences.getConsentState` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/Preferences.ts:105 Preferences.streamConsent` | member | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/Preferences.ts:121 Preferences.streamPreferences` | member | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/ServerConfiguration.ts:21 ServerConfigurationError.constructor` | member | `ServerConfigurationError.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/ServerConfiguration.ts:23 ServerConfigurationError.code` | member | `ServerConfigurationError.code` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/ServerConfiguration.ts:24 ServerConfigurationError.message` | member | `ServerConfigurationError.message` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/ServerConfiguration.ts:25 ServerConfigurationError.options` | member | `ServerConfigurationError.options` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/ServerConfiguration.ts:38 ConfigurationUnavailableError.constructor` | member | `ConfigurationUnavailableError.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/ServerConfiguration.ts:50 ConfigurationInvalidError.constructor` | member | `ConfigurationInvalidError.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/ServerConfiguration.ts:62 BackendMismatchError.constructor` | member | `BackendMismatchError.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/ServerConfiguration.ts:70 ClientVersionTooOldError.constructor` | member | `ClientVersionTooOldError.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/ServerConfiguration.ts:81 AuthRequiredError.constructor` | member | `AuthRequiredError.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/ServerConfiguration.ts:93 ChainNotAcceptedError.constructor` | member | `ChainNotAcceptedError.constructor` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:9 AsyncStreamProxy` | re-export | `AsyncStreamProxy` | static runtime | 11.4 Node | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/node/src/index.ts:9 ResolveValue` | re-export | `ResolveValue` | static runtime | 11.4 Node | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/node/src/index.ts:10 CodecRegistry` | re-export | `CodecRegistry` | static runtime | 11.4 Node | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/node/src/index.ts:11 Client` | re-export | `Client` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:12 NotificationChannel` | re-export | `NotificationChannel` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:12 NotificationConfig` | re-export | `NotificationConfig` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:12 NotificationError` | re-export | `NotificationError` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:12 NotificationOverride` | re-export | `NotificationOverride` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:12 NotificationState` | re-export | `NotificationState` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:19 Conversation` | re-export | `Conversation` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:20 Conversations` | re-export | `Conversations` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:21 DecodedMessage` | re-export | `Message` | alias | 11.4 Node; 19.2 | Deprecated alias for one major release (11.4, 19.5). |
| `sdks/node/src/index.ts:22 MessageAcknowledgement` | re-export | `MessageAcknowledgement` | static runtime | 11.4 Node | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/node/src/index.ts:22 MessageDelivery` | re-export | `MessageDelivery` | static runtime | 11.4 Node | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/node/src/index.ts:22 MessageReaderSource` | re-export | `MessageReaderSource` | static runtime | 11.4 Node | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/node/src/index.ts:22 MessageStream` | re-export | `MessageStream` | static runtime | 11.4 Node | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/node/src/index.ts:36 DebugInformation` | re-export | `Diagnostics` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/index.ts:37 Dm` | re-export | `Dm` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:38 Group` | re-export | `Group` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:39 Preferences` | re-export | `Preferences` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:40 AuthRequiredError` | re-export | `AuthRequiredError` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:40 BackendMismatchError` | re-export | `BackendMismatchError` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:40 ChainNotAcceptedError` | re-export | `ChainNotAcceptedError` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:40 ClientVersionTooOldError` | re-export | `ClientVersionTooOldError` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:40 ConfigurationInvalidError` | re-export | `ConfigurationInvalidError` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:40 ConfigurationUnavailableError` | re-export | `ConfigurationUnavailableError` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:40 ServerConfigurationError` | re-export | `ServerConfigurationError` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:40 throwServerConfigurationError` | re-export | `throwServerConfigurationError` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/index.ts:40 toServerConfigurationError` | re-export | `toServerConfigurationError` | generated | 11.4 Node | Facade schema or generated record (11.1-11.4). |
| `sdks/node/src/types.ts:31 Credential` | type | `Credential` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/types.ts:40 AuthCallback` | type | `AuthCallback` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/types.ts:42 NetworkOptions` | type | `NetworkOptions` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/types.ts:56 DeviceSyncOptions` | type | `DeviceSyncOptions` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/types.ts:66 StorageOptions` | type | `StorageOptions` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/types.ts:117 ContentOptions` | type | `ContentOptions` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/types.ts:124 OtherOptions` | type | `OtherOptions` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/types.ts:201 ClientOptions` | type | `ClientOptions` | static runtime | 11.4 Node | Host options wrapper accepts codecs or callbacks (11.1, 20.1). |
| `sdks/node/src/types.ts:213 DistributiveOmit` | type | `DistributiveOmit` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/types.ts:217 EnrichedReply` | type | `EnrichedReply` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/types.ts:224 BuiltInContentTypes` | type | `BuiltInContentTypes` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/types.ts:239 ExtractCodecContentTypes` | type | `ExtractCodecContentTypes` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/createBackend.ts:11 createBackend` | const | `Backend.connect()` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/utils/errors.ts:1 InboxReassignError` | class | `InboxReassignError` | generated | 11.4 Node | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/node/src/utils/errors.ts:2 InboxReassignError.constructor` | member | `InboxReassignError.constructor` | generated | 11.4 Node | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/node/src/utils/errors.ts:9 AccountAlreadyAssociatedError` | class | `AccountAlreadyAssociatedError` | generated | 11.4 Node | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/node/src/utils/errors.ts:10 AccountAlreadyAssociatedError.constructor` | member | `AccountAlreadyAssociatedError.constructor` | generated | 11.4 Node | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/node/src/utils/errors.ts:15 MissingContentTypeError` | class | `MissingContentTypeError` | generated | 11.4 Node | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/node/src/utils/errors.ts:16 MissingContentTypeError.constructor` | member | `MissingContentTypeError.constructor` | generated | 11.4 Node | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/node/src/utils/errors.ts:21 SignerUnavailableError` | class | `SignerUnavailableError` | generated | 11.4 Node | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/node/src/utils/errors.ts:22 SignerUnavailableError.constructor` | member | `SignerUnavailableError.constructor` | generated | 11.4 Node | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/node/src/utils/errors.ts:29 ClientNotInitializedError` | class | `ClientNotInitializedError` | generated | 11.4 Node | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/node/src/utils/errors.ts:30 ClientNotInitializedError.constructor` | member | `ClientNotInitializedError.constructor` | generated | 11.4 Node | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/node/src/utils/errors.ts:37 StreamFailedError` | class | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/utils/errors.ts:38 StreamFailedError.constructor` | member | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/utils/errors.ts:44 StreamInvalidRetryAttemptsError` | class | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/utils/errors.ts:45 StreamInvalidRetryAttemptsError.constructor` | member | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/utils/inboxId.ts:8 generateInboxId` | const | `Client.inboxID()` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/utils/inboxId.ts:15 getInboxIdForIdentifier` | const | `Client.inboxID()` | alias | 11.4 Node; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/node/src/utils/messages.ts:18 isReaction` | const | `isReaction` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:22 isReply` | const | `isReply` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:27 isTextReply` | const | `isTextReply` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:32 isText` | const | `isText` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:35 isRemoteAttachment` | const | `isRemoteAttachment` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:41 isAttachment` | const | `isAttachment` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:47 isMultiRemoteAttachment` | const | `isMultiRemoteAttachment` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:53 isTransactionReference` | const | `isTransactionReference` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:59 isGroupUpdated` | const | `isGroupUpdated` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:65 isReadReceipt` | const | `isReadReceipt` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:71 isLeaveRequest` | const | `isLeaveRequest` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:77 isWalletSendCalls` | const | `isWalletSendCalls` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:83 isIntent` | const | `isIntent` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:87 isActions` | const | `isActions` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/messages.ts:91 isMarkdown` | const | `isMarkdown` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/signer.ts:3 SignMessage` | type | `SignMessage` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/signer.ts:4 GetIdentifier` | type | `GetIdentifier` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/signer.ts:5 GetChainId` | type | `GetChainID` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/signer.ts:6 GetBlockNumber` | type | `GetBlockNumber` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/signer.ts:8 Signer` | type | `Signer` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/signer.ts:22 EOASigner` | type | `EOASigner` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/signer.ts:23 SCWSigner` | type | `SCWSigner` | static runtime | 11.4 Node | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/node/src/utils/streamFailure.ts:4 StreamFailureCause` | type | `StreamFailureCause` | generated | 11.4 Node | Typed stream failure details (5, 11.4). |
| `sdks/node/src/utils/streamFailure.ts:18 UnfinishedStreamTopic` | type | `UnfinishedStreamTopic` | generated | 11.4 Node | Typed stream failure details (5, 11.4). |
| `sdks/node/src/utils/streamFailure.ts:29 StreamBarrierFailure` | type | `StreamBarrierFailure` | generated | 11.4 Node | Typed stream failure details (5, 11.4). |
| `sdks/node/src/utils/streamFailure.ts:34 StreamFailureDetails` | type | `StreamFailureDetails` | generated | 11.4 Node | Typed stream failure details (5, 11.4). |
| `sdks/node/src/utils/streamFailure.ts:55 getStreamFailureDetails` | const | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/utils/streams.ts:10 DEFAULT_RETRY_DELAY` | const | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/utils/streams.ts:11 DEFAULT_RETRY_ATTEMPTS` | const | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/utils/streams.ts:39 StreamOptions` | type | `StreamOptions` | static runtime | 11.4 Node | Host stream adapter and options (5, 11.4). |
| `sdks/node/src/utils/streams.ts:85 MessageStreamOptions` | type | `MessageStreamOptions` | static runtime | 11.4 Node | Host stream adapter and options (5, 11.4). |
| `sdks/node/src/utils/streams.ts:90 StreamCallback` | type | `StreamCallback` | static runtime | 11.4 Node | Host stream adapter and options (5, 11.4). |
| `sdks/node/src/utils/streams.ts:95 StreamFunction` | type | `StreamFunction` | static runtime | 11.4 Node | Host stream adapter and options (5, 11.4). |
| `sdks/node/src/utils/streams.ts:100 StreamValueMutator` | type | `StreamValueMutator` | static runtime | 11.4 Node | Host stream adapter and options (5, 11.4). |
| `sdks/node/src/utils/streams.ts:121 createStream` | const | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/utils/validation.ts:1 HexString` | type | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/utils/validation.ts:3 isHexString` | function | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |
| `sdks/node/src/utils/validation.ts:7 validHex` | function | — | approved removal | 11.4 Node | Removal or replacement approved in 11.4 and 19. |

## Browser

| Current export | Kind | Final name | Status | Design ref | Notes |
| --- | --- | --- | --- | --- | --- |
| `@xmtp/wasm-bindings:16 DeliveryCursor` | binding re-export | `DeliveryCursor` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:16 MessageCatchUp` | binding re-export | `MessageCatchUp` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:16 MessageCatchUpGeneration` | binding re-export | `MessageCatchUpGeneration` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:16 MessageHistorySnapshot` | binding re-export | `MessageHistorySnapshot` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:16 MessageTopicStatus` | binding re-export | `MessageTopicStatus` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Action` | binding re-export | `Action` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Actions` | binding re-export | `Actions` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 ApiStats` | binding re-export | `ApiStats` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 ArchiveMetadata` | binding re-export | `ArchiveMetadata` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 ArchiveOptions` | binding re-export | `ArchiveOptions` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Attachment` | binding re-export | `Attachment` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 AuthConfiguration` | binding re-export | `AuthConfiguration` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Backend` | binding re-export | `Backend` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 BackendBuilder` | binding re-export | `BackendBuilder` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Consent` | binding re-export | `Consent` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 ConversationDebugInfo` | binding re-export | `ConversationDebugInfo` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 ConversationListItem` | binding re-export | `ConversationListItem` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 CreateDmOptions` | binding re-export | `CreateDmOptions` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 CreateGroupOptions` | binding re-export | `CreateGroupOptions` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Cursor` | binding re-export | `Cursor` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 EncryptedAttachment` | binding re-export | `EncryptedAttachment` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 GroupMember` | binding re-export | `GroupMember` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 GroupMetadata` | binding re-export | `GroupMetadata` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 GroupPermissions` | binding re-export | `GroupPermissions` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 GroupSyncSummary` | binding re-export | `GroupSyncSummary` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 GroupUpdated` | binding re-export | `GroupUpdated` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 HmacKey` | binding re-export | `HmacKey` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Identifier` | binding re-export | `Identifier` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 IdentityStats` | binding re-export | `IdentityStats` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Inbox` | binding re-export | `Inbox` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 InboxState` | binding re-export | `InboxState` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Installation` | binding re-export | `Installation` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Intent` | binding re-export | `Intent` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 KeyPackageStatus` | binding re-export | `KeyPackageStatus` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 LeaveRequest` | binding re-export | `LeaveRequest` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Lifetime` | binding re-export | `Lifetime` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 LimitsConfiguration` | binding re-export | `LimitsConfiguration` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 ListConversationsOptions` | binding re-export | `ListConversationsOptions` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 ListMessagesOptions` | binding re-export | `ListMessagesOptions` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 LogOptions` | binding re-export | `LogOptions` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Message` | binding re-export | `Message` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 MessageDisappearingSettings` | binding re-export | `MessageDisappearingSettings` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 MetadataFieldChange` | binding re-export | `MetadataFieldChange` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 MlsConfiguration` | binding re-export | `MlsConfiguration` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 MultiRemoteAttachment` | binding re-export | `MultiRemoteAttachment` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 PermissionPolicySet` | binding re-export | `PermissionPolicySet` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Reaction` | binding re-export | `Reaction` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 ReadReceipt` | binding re-export | `ReadReceipt` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 RemoteAttachment` | binding re-export | `RemoteAttachment` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 Reply` | binding re-export | `Reply` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 RetentionConfiguration` | binding re-export | `RetentionConfiguration` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 SendMessageOpts` | binding re-export | `SendMessageOpts` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 SendOpts` | binding re-export | `SendOpts` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 ServerConfiguration` | binding re-export | `ServerConfiguration` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 SignatureRequestHandle` | binding re-export | `SignatureRequestHandle` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 SigningKeyDescription` | binding re-export | `SigningKeyDescription` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 TransactionMetadata` | binding re-export | `TransactionMetadata` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 TransactionReference` | binding re-export | `TransactionReference` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 UserPreferenceUpdate` | binding re-export | `UserPreferenceUpdate` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 WalletCall` | binding re-export | `WalletCall` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 WalletSendCalls` | binding re-export | `WalletSendCalls` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 WorkerConfigOptions` | binding re-export | `WorkerConfigOptions` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:33 WorkerIntervalOverride` | binding re-export | `WorkerIntervalOverride` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 ActionStyle` | binding re-export | `ActionStyle` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 BackupElementSelectionOption` | binding re-export | `BackupElementSelectionOption` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 ConsentEntityType` | binding re-export | `ConsentEntityType` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 ConsentState` | binding re-export | `ConsentState` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 ContentType` | binding re-export | `ContentType` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 ConversationType` | binding re-export | `ConversationType` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 DeliveryStatus` | binding re-export | `DeliveryStatus` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 GroupMembershipState` | binding re-export | `GroupMembershipState` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 GroupMessageKind` | binding re-export | `GroupMessageKind` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 GroupPermissionsOptions` | binding re-export | `GroupPermissionsOptions` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 IdentifierKind` | binding re-export | `IdentifierKind` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 ListConversationsOrderBy` | binding re-export | `ListConversationsOrderBy` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 LogLevel` | binding re-export | `LogLevel` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 MessageSortBy` | binding re-export | `MessageSortBy` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 MetadataField` | binding re-export | `MetadataField` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 PermissionLevel` | binding re-export | `PermissionLevel` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 PermissionPolicy` | binding re-export | `PermissionPolicy` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 PermissionUpdateType` | binding re-export | `PermissionUpdateType` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 ReactionAction` | binding re-export | `ReactionAction` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 ReactionSchema` | binding re-export | `ReactionSchema` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 SortDirection` | binding re-export | `SortDirection` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `@xmtp/wasm-bindings:93 WorkerKind` | binding re-export | `WorkerKind` | generated | 11.4 Browser | Binding export supplied by the facade generator (11.4). |
| `sdks/browser/src/Client.ts:105 Client.constructor` | member | `Client.constructor` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:160 Client.init` | member | `Client.init` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:189 Client.close` | member | `end()` | alias | 11.4 Browser; 19.2 | Async client and reader shutdown uses end() (plan Decisions, Section 20.8). |
| `sdks/browser/src/Client.ts:224 Client.create` | member | `Client.create` | static runtime | 11.4 Browser | Host wrapper owns codecs and the client registry (11.1, 11.7). |
| `sdks/browser/src/Client.ts:225 Client.signer` | member | `Client.signer` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:226 Client.options` | member | `Client.options` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:259 Client.build` | member | `Client.build` | static runtime | 11.4 Browser | Host wrapper owns codecs and the client registry (11.1, 11.7). |
| `sdks/browser/src/Client.ts:260 Client.identifier` | member | `Client.identifier` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:299 Client.isReady` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Client.ts:306 Client.inboxId` | member | `inboxID` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/Client.ts:313 Client.accountIdentifier` | member | `Client.accountIdentifier` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:320 Client.installationId` | member | `installationID` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/Client.ts:327 Client.installationIdBytes` | member | `Client.installationIdBytes` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:334 Client.conversations` | member | `Client.conversations` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:341 Client.debugInformation` | member | `diagnostics` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/Client.ts:348 Client.preferences` | member | `Client.preferences` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:355 Client.libxmtpVersion` | member | `Client.libxmtpVersion` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:362 Client.appVersion` | member | `Client.appVersion` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:369 Client.env` | member | `storage.label` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/Client.ts:384 Client.unsafe_createInboxSignatureText` | member | `Client.unsafe_createInboxSignatureText` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:404 Client.unsafe_addAccountSignatureText` | member | `Client.unsafe_addAccountSignatureText` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:405 Client.newIdentifier` | member | `Client.newIdentifier` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:406 Client.allowInboxReassign` | member | `Client.allowInboxReassign` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:430 Client.unsafe_removeAccountSignatureText` | member | `Client.unsafe_removeAccountSignatureText` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:449 Client.unsafe_revokeAllOtherInstallationsSignatureText` | member | `Client.unsafe_revokeAllOtherInstallationsSignatureText` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:471 Client.unsafe_revokeInstallationsSignatureText` | member | `Client.unsafe_revokeInstallationsSignatureText` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:491 Client.unsafe_changeRecoveryIdentifierSignatureText` | member | `Client.unsafe_changeRecoveryIdentifierSignatureText` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:512 Client.unsafe_applySignatureRequest` | member | `Client.unsafe_applySignatureRequest` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:514 Client.signatureRequestId` | member | `Client.signatureRequestID` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:529 Client.register` | member | `Client.register` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:570 Client.unsafe_addAccount` | member | `Client.unsafe_addAccount` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:571 Client.newAccountSigner` | member | `Client.newAccountSigner` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:613 Client.removeAccount` | member | `Client.removeAccount` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:637 Client.revokeAllOtherInstallations` | member | `Client.revokeAllOtherInstallations` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:667 Client.revokeInstallations` | member | `Client.revokeInstallations` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:688 Client.installationIds` | member | `Client.installationIDs` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:689 Client.optionsOrBackend` | member | `Client.optionsOrBackend` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:696 Client.fetchInboxStates` | member | `Client.fetchInboxStates` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:697 Client.inboxIds` | member | `Client.inboxIDs` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:712 Client.changeRecoveryIdentifier` | member | `Client.changeRecoveryIdentifier` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:734 Client.isRegistered` | member | `Client.isRegistered` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:744 Client.canMessage` | member | `Client.canMessage` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:754 Client.fetchLatestInboxUpdatesCount` | member | `Client.fetchLatestInboxUpdatesCount` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:770 Client.fetchOwnInboxUpdatesCount` | member | `Client.fetchOwnInboxUpdatesCount` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:776 Client.identifiers` | member | `Client.identifiers` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:824 Client.fetchInboxIdByIdentifier` | member | `Client.fetchInboxIdByIdentifier` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:834 Client.signWithInstallationKey` | member | `Client.signWithInstallationKey` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:847 Client.verifySignedWithInstallationKey` | member | `Client.verifySignedWithInstallationKey` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:848 Client.signatureText` | member | `Client.signatureText` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:849 Client.signatureBytes` | member | `Client.signatureBytes` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:865 Client.verifySignedWithPublicKey` | member | `Client.verifySignedWithPublicKey` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:868 Client.publicKey` | member | `Client.publicKey` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:884 Client.fetchKeyPackageStatuses` | member | `Client.fetchKeyPackageStatuses` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:910 Client.createArchive` | member | `archives.exportToBytes()` | alias | 11.4 Browser; 19.2 | Browser archives use bytes (11.4 Browser, 19.25). |
| `sdks/browser/src/Client.ts:911 Client.key` | member | `Client.key` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:912 Client.opts` | member | `Client.opts` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:929 Client.importArchive` | member | `archives.importFromBytes()` | alias | 11.4 Browser; 19.2 | Browser archives use bytes (11.4 Browser, 19.25). |
| `sdks/browser/src/Client.ts:943 Client.archiveMetadata` | member | `archives.metadataFromBytes()` | alias | 11.4 Browser; 19.2 | Browser archives use bytes (11.4 Browser, 19.25). |
| `sdks/browser/src/Client.ts:944 Client.data` | member | `Client.data` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:958 Client.syncAllDeviceSyncGroups` | member | `Client.syncAllDeviceSyncGroups` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:973 Client.serverConfiguration` | member | `Client.serverConfiguration` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:988 Client.refreshServerConfiguration` | member | `Client.refreshServerConfiguration` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:1005 Client.fetchServerConfiguration` | member | `Client.fetchServerConfiguration` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Client.ts:1006 Client.optionsOrUrl` | member | `Client.optionsOrUrl` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/CodecRegistry.ts:10 CodecRegistry.constructor` | member | `CodecRegistry.constructor` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/CodecRegistry.ts:22 CodecRegistry.getCodec` | member | `CodecRegistry.getCodec` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/Conversation.ts:54 Conversation.constructor` | member | `Conversation.constructor` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:55 Conversation.worker` | member | `Conversation.worker` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:56 Conversation.codecRegistry` | member | `Conversation.codecRegistry` | static runtime | 11.4 Browser | Per-client codec registry stays in the host runtime (11.4). |
| `sdks/browser/src/Conversation.ts:57 Conversation.id` | member | `Conversation.id` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:58 Conversation.data` | member | `Conversation.data` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:76 Conversation.addedByInboxId` | member | `Conversation.addedByInboxID` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:80 Conversation.createdAtNs` | member | `Conversation.createdAt.ns` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:84 Conversation.createdAt` | member | `Conversation.createdAt` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:88 Conversation.metadata` | member | — | approved removal | 11.4 Browser | Immutable kind and creatorInboxID replace metadata() (11.4). |
| `sdks/browser/src/Conversation.ts:92 Conversation.topic` | member | `Conversation.topic` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:96 Conversation.lastMessage` | member | `Conversation.lastMessage` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:105 Conversation.isActive` | member | `state().isActive` | generated | 11.4 Browser | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/browser/src/Conversation.ts:116 Conversation.members` | member | `Conversation.members` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:127 Conversation.sync` | member | `Conversation.sync` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:140 Conversation.publishMessages` | member | `Conversation.publishMessages` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:152 Conversation.processStreamedMessage` | member | `Conversation.processStreamedMessage` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:172 Conversation.send` | member | `Conversation.send` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:187 Conversation.sendText` | member | `Conversation.sendText` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:202 Conversation.sendMarkdown` | member | `Conversation.sendMarkdown` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:217 Conversation.sendReaction` | member | `Conversation.sendReaction` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:231 Conversation.sendReadReceipt` | member | `Conversation.sendReadReceipt` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:245 Conversation.sendReply` | member | `Conversation.sendReply` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:260 Conversation.sendTransactionReference` | member | `Conversation.sendTransactionReference` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:261 Conversation.transactionReference` | member | `Conversation.transactionReference` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:262 Conversation.opts` | member | `Conversation.opts` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:278 Conversation.sendWalletSendCalls` | member | `Conversation.sendWalletSendCalls` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:293 Conversation.sendActions` | member | `Conversation.sendActions` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:308 Conversation.sendIntent` | member | `Conversation.sendIntent` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:323 Conversation.sendAttachment` | member | `Conversation.sendAttachment` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:338 Conversation.sendMultiRemoteAttachment` | member | `Conversation.sendMultiRemoteAttachment` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:339 Conversation.multiRemoteAttachment` | member | `Conversation.multiRemoteAttachment` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:356 Conversation.sendRemoteAttachment` | member | `Conversation.sendRemoteAttachment` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:357 Conversation.remoteAttachment` | member | `Conversation.remoteAttachment` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:373 Conversation.messages` | member | `Conversation.messages` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:391 Conversation.countMessages` | member | `Conversation.countMessages` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:392 Conversation.options` | member | `Conversation.options` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:406 Conversation.consentState` | member | `state().consentState` | generated | 11.4 Browser | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/browser/src/Conversation.ts:418 Conversation.updateConsentState` | member | `Conversation.updateConsentState` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:430 Conversation.messageDisappearingSettings` | member | `state().disappearingSettings` | generated | 11.4 Browser | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/browser/src/Conversation.ts:443 Conversation.updateMessageDisappearingSettings` | member | `updateDisappearingSettings()` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/Conversation.ts:459 Conversation.removeMessageDisappearingSettings` | member | `updateDisappearingSettings(null)` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/Conversation.ts:473 Conversation.isMessageDisappearingEnabled` | member | `state().isDisappearingEnabled` | generated | 11.4 Browser | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/browser/src/Conversation.ts:488 Conversation.stream` | member | `Conversation.stream` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:491 Conversation.DecodedMessage` | member | `Message` | alias | 11.4 Browser; 19.2 | Deprecated alias for one major release (11.4, 19.5). |
| `sdks/browser/src/Conversation.ts:514 Conversation.messageHistorySnapshot` | member | `Conversation.messageHistorySnapshot` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:521 Conversation.beginningDeliveryCursor` | member | `Conversation.beginningDeliveryCursor` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:525 Conversation.pausedForVersion` | member | `state().pausedForVersion` | generated | 11.4 Browser | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/browser/src/Conversation.ts:536 Conversation.hmacKeys` | member | `Conversation.hmacKeys` | generated | 11.4 Browser | Returns ID-keyed entry records, not a map (plan Decisions, Section 20.7). |
| `sdks/browser/src/Conversation.ts:547 Conversation.debugInfo` | member | `Conversation.debugInfo` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversation.ts:559 Conversation.lastReadTimes` | member | `Conversation.lastReadTimes` | generated | 11.4 Browser | Returns ID-keyed entry records, not a map (plan Decisions, Section 20.7). |
| `sdks/browser/src/Conversations.ts:49 Conversations.constructor` | member | `Conversations.constructor` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:50 Conversations.client` | member | `Conversations.client` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:51 Conversations.worker` | member | `Conversations.worker` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:52 Conversations.codecRegistry` | member | `Conversations.codecRegistry` | static runtime | 11.4 Browser | Per-client codec registry stays in the host runtime (11.4). |
| `sdks/browser/src/Conversations.ts:59 Conversations.topic` | member | `Conversations.topic` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:70 Conversations.sync` | member | `Conversations.sync` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:81 Conversations.syncAll` | member | `Conversations.syncAll` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:93 Conversations.getConversationById` | member | `getByID()` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/Conversations.ts:129 Conversations.getMessageById` | member | `Conversations.getMessageByID` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:144 Conversations.getDmByInboxId` | member | `Conversations.getDmByInboxID` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:159 Conversations.fetchDmByIdentifier` | member | `getDmByIdentity()` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/Conversations.ts:175 Conversations.list` | member | `Conversations.list` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:210 Conversations.listGroups` | member | `Conversations.listGroups` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:211 Conversations.options` | member | `Conversations.options` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:237 Conversations.listDms` | member | `Conversations.listDms` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:259 Conversations.createGroupOptimistic` | member | `Conversations.createGroupOptimistic` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:282 Conversations.createGroupWithIdentifiers` | member | `createGroupWithIdentities()` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/Conversations.ts:283 Conversations.identifiers` | member | `Conversations.identifiers` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:309 Conversations.createGroup` | member | `Conversations.createGroup` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:333 Conversations.createDmWithIdentifier` | member | `createDmWithIdentity()` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/Conversations.ts:334 Conversations.identifier` | member | `Conversations.identifier` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:360 Conversations.createDm` | member | `Conversations.createDm` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:379 Conversations.hmacKeys` | member | `Conversations.hmacKeys` | generated | 11.4 Browser | Returns ID-keyed entry records, not a map (plan Decisions, Section 20.7). |
| `sdks/browser/src/Conversations.ts:390 Conversations.stream` | member | `Conversations.stream` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:456 Conversations.streamGroups` | member | `Conversations.streamGroups` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:471 Conversations.streamDms` | member | `Conversations.streamDms` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:489 Conversations.streamAllMessages` | member | `Conversations.streamAllMessages` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:492 Conversations.DecodedMessage` | member | `Message` | alias | 11.4 Browser; 19.2 | Deprecated alias for one major release (11.4, 19.5). |
| `sdks/browser/src/Conversations.ts:522 Conversations.messageHistorySnapshot` | member | `Conversations.messageHistorySnapshot` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:523 Conversations.limit` | member | `Conversations.limit` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:533 Conversations.beginningDeliveryCursor` | member | `Conversations.beginningDeliveryCursor` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:544 Conversations.streamAllGroupMessages` | member | `Conversations.streamAllGroupMessages` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:567 Conversations.streamAllDmMessages` | member | `Conversations.streamAllDmMessages` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:591 Conversations.streamMessageDeletions` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Conversations.ts:593 Conversations.StreamOptions` | member | `Conversations.StreamOptions` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Conversations.ts:626 Conversations.streamDeletedMessages` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/DebugInformation.ts:12 DebugInformation.constructor` | member | `DebugInformation.constructor` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/DebugInformation.ts:16 DebugInformation.apiStatistics` | member | `DebugInformation.apiStatistics` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/DebugInformation.ts:20 DebugInformation.apiIdentityStatistics` | member | `DebugInformation.apiIdentityStatistics` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/DebugInformation.ts:24 DebugInformation.apiAggregateStatistics` | member | `DebugInformation.apiAggregateStatistics` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/DebugInformation.ts:28 DebugInformation.clearAllStatistics` | member | `DebugInformation.clearAllStatistics` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/DecodedMessage.ts:179 DecodedMessage.deliveryCursor` | member | `DecodedMessage.deliveryCursor` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:180 DecodedMessage.content` | member | `DecodedMessage.content` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:181 DecodedMessage.contentType` | member | `DecodedMessage.contentType` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:182 DecodedMessage.conversationId` | member | `DecodedMessage.conversationID` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:183 DecodedMessage.deliveryStatus` | member | `DecodedMessage.deliveryStatus` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:184 DecodedMessage.expiresAtNs` | member | `DecodedMessage.expiresAt.ns` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:185 DecodedMessage.expiresAt` | member | `DecodedMessage.expiresAt` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:186 DecodedMessage.fallback` | member | `DecodedMessage.fallback` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:187 DecodedMessage.id` | member | `DecodedMessage.id` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:188 DecodedMessage.kind` | member | `DecodedMessage.kind` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:189 DecodedMessage.numReplies` | member | `DecodedMessage.numReplies` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:190 DecodedMessage.reactions` | member | `DecodedMessage.reactions` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:191 DecodedMessage.senderInboxId` | member | `DecodedMessage.senderInboxID` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:192 DecodedMessage.sentAt` | member | `DecodedMessage.sentAt` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:193 DecodedMessage.sentAtNs` | member | `DecodedMessage.sentAt.ns` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/DecodedMessage.ts:195 DecodedMessage.constructor` | member | `DecodedMessage.constructor` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/Dm.ts:25 Dm.constructor` | member | `Dm.constructor` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Dm.ts:26 Dm.worker` | member | `Dm.worker` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Dm.ts:27 Dm.codecRegistry` | member | `Dm.codecRegistry` | static runtime | 11.4 Browser | Per-client codec registry stays in the host runtime (11.4). |
| `sdks/browser/src/Dm.ts:28 Dm.id` | member | `Dm.id` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Dm.ts:29 Dm.data` | member | `Dm.data` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Dm.ts:42 Dm.peerInboxId` | member | `peerInboxID` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/Dm.ts:48 Dm.duplicateDms` | member | `Dm.duplicateDms` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:46 Group.constructor` | member | `Group.constructor` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:47 Group.worker` | member | `Group.worker` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:48 Group.codecRegistry` | member | `Group.codecRegistry` | static runtime | 11.4 Browser | Per-client codec registry stays in the host runtime (11.4). |
| `sdks/browser/src/Group.ts:49 Group.id` | member | `Group.id` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:50 Group.data` | member | `Group.data` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:63 Group.sync` | member | `Group.sync` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:72 Group.name` | member | `state().name` | generated | 11.4 Browser | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/browser/src/Group.ts:81 Group.updateName` | member | `Group.updateName` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:92 Group.imageUrl` | member | `state().imageUrl` | generated | 11.4 Browser | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/browser/src/Group.ts:101 Group.updateImageUrl` | member | `Group.updateImageUrl` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:112 Group.description` | member | `state().description` | generated | 11.4 Browser | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/browser/src/Group.ts:121 Group.updateDescription` | member | `Group.updateDescription` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:132 Group.appData` | member | `state().appData` | generated | 11.4 Browser | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/browser/src/Group.ts:141 Group.updateAppData` | member | `Group.updateAppData` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:152 Group.admins` | member | `state().admins` | generated | 11.4 Browser | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/browser/src/Group.ts:159 Group.superAdmins` | member | `state().superAdmins` | generated | 11.4 Browser | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/browser/src/Group.ts:168 Group.listAdmins` | member | `Group.listAdmins` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:181 Group.listSuperAdmins` | member | `Group.listSuperAdmins` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:194 Group.permissions` | member | `state().permissions` | generated | 11.4 Browser | One conversation-state read (11.2, 11.4, 19.4). |
| `sdks/browser/src/Group.ts:207 Group.updatePermission` | member | `Group.updatePermission` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:208 Group.permissionType` | member | `Group.permissionType` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:209 Group.policy` | member | `Group.policy` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:210 Group.metadataField` | member | `Group.metadataField` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:226 Group.isAdmin` | member | `Group.isAdmin` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:237 Group.isSuperAdmin` | member | `Group.isSuperAdmin` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:247 Group.addMembersByIdentifiers` | member | `Group.addMembersByIdentifiers` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:259 Group.addMembers` | member | `Group.addMembers` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:271 Group.removeMembersByIdentifiers` | member | `Group.removeMembersByIdentifiers` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:283 Group.removeMembers` | member | `Group.removeMembers` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:295 Group.addAdmin` | member | `Group.addAdmin` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:307 Group.removeAdmin` | member | `Group.removeAdmin` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:319 Group.addSuperAdmin` | member | `Group.addSuperAdmin` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:331 Group.removeSuperAdmin` | member | `Group.removeSuperAdmin` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:341 Group.requestRemoval` | member | `Group.requestRemoval` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Group.ts:352 Group.isPendingRemoval` | member | `Group.isPendingRemoval` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/MessageStream.ts:38 MessageStream.message` | member | `MessageStream.message` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:39 MessageStream.cursor` | member | `MessageStream.cursor` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:49 MessageStream.constructor` | member | `MessageStream.constructor` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:50 MessageStream.reader` | member | `MessageStream.reader` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:51 MessageStream.convert` | member | `MessageStream.convert` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:55 MessageStream.options` | member | `MessageStream.options` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:67 MessageStream.isDone` | member | `MessageStream.isDone` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:70 MessageStream.deliveredCursor` | member | `MessageStream.deliveredCursor` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:78 MessageStream.next` | member | `MessageStream.next` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:177 MessageStream.pending` | member | `MessageStream.pending` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:216 MessageStream.end` | member | `MessageStream.end` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:217 MessageStream.updateScope` | member | `MessageStream.updateScope` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:218 MessageStream.updateFilter` | member | `MessageStream.updateFilter` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:219 MessageStream.conversationType` | member | `MessageStream.conversationType` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:220 MessageStream.consentStates` | member | `MessageStream.consentStates` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:222 MessageStream.catchUpSnapshot` | member | `MessageStream.catchUpSnapshot` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/MessageStream.ts:223 MessageStream.catchUpChanged` | member | `MessageStream.catchUpChanged` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/Opfs.ts:8 Opfs.constructor` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Opfs.ts:16 Opfs.init` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Opfs.ts:22 Opfs.close` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Opfs.ts:26 Opfs.create` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Opfs.ts:32 Opfs.listFiles` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Opfs.ts:36 Opfs.fileCount` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Opfs.ts:40 Opfs.poolCapacity` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Opfs.ts:44 Opfs.fileExists` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Opfs.ts:48 Opfs.deleteFile` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Opfs.ts:52 Opfs.exportDb` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Opfs.ts:61 Opfs.importDb` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Opfs.ts:65 Opfs.clearAll` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Preferences.ts:30 Preferences.constructor` | member | `Preferences.constructor` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Preferences.ts:34 Preferences.sync` | member | `Preferences.sync` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Preferences.ts:43 Preferences.inboxState` | member | `Preferences.inboxState` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Preferences.ts:54 Preferences.fetchInboxState` | member | `Preferences.fetchInboxState` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Preferences.ts:67 Preferences.getInboxStates` | member | `Preferences.getInboxStates` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Preferences.ts:80 Preferences.fetchInboxStates` | member | `Preferences.fetchInboxStates` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Preferences.ts:93 Preferences.setConsentStates` | member | `Preferences.setConsentStates` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Preferences.ts:106 Preferences.getConsentState` | member | `Preferences.getConsentState` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Preferences.ts:107 Preferences.entityType` | member | `Preferences.entityType` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Preferences.ts:108 Preferences.entity` | member | `Preferences.entity` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/Preferences.ts:122 Preferences.streamConsent` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/Preferences.ts:152 Preferences.streamPreferences` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/index.ts:1 Client` | re-export | `Client` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/index.ts:2 CodecRegistry` | re-export | `CodecRegistry` | static runtime | 11.4 Browser | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/browser/src/index.ts:3 Opfs` | re-export | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/index.ts:4 Conversations` | re-export | `Conversations` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/index.ts:5 Conversation` | re-export | `Conversation` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/index.ts:6 Dm` | re-export | `Dm` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/index.ts:7 Group` | re-export | `Group` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/index.ts:8 DecodedMessage` | re-export | `Message` | alias | 11.4 Browser; 19.2 | Deprecated alias for one major release (11.4, 19.5). |
| `sdks/browser/src/index.ts:9 MessageAcknowledgement` | re-export | `MessageAcknowledgement` | static runtime | 11.4 Browser | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/browser/src/index.ts:9 MessageDelivery` | re-export | `MessageDelivery` | static runtime | 11.4 Browser | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/browser/src/index.ts:9 MessageReaderSource` | re-export | `MessageReaderSource` | static runtime | 11.4 Browser | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/browser/src/index.ts:9 MessageStream` | re-export | `MessageStream` | static runtime | 11.4 Browser | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/browser/src/index.ts:23 DebugInformation` | re-export | `Diagnostics` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/index.ts:24 Preferences` | re-export | `Preferences` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/index.ts:25 createBackend` | re-export | `Backend.connect()` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/index.ts:26 fetchServerConfiguration` | re-export | `fetchServerConfiguration` | generated | 11.4 Browser | Facade schema or generated record (11.1-11.4). |
| `sdks/browser/src/index.ts:27 generateInboxId` | re-export | `Client.inboxID()` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/index.ts:27 getInboxIdForIdentifier` | re-export | `Client.inboxID()` | alias | 11.4 Browser; 19.2 | Deprecated rename for one major release (11.4, 19.2). |
| `sdks/browser/src/index.ts:28 metadataFieldName` | re-export | `metadataFieldName` | static runtime | 2; open | Proposed host helper; the design does not name this export. |
| `sdks/browser/src/index.ts:32 AsyncStreamProxy` | re-export | `AsyncStreamProxy` | static runtime | 11.4 Browser | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/browser/src/index.ts:32 ResolveValue` | re-export | `ResolveValue` | static runtime | 11.4 Browser | Host codec or stream adapter (2, 5, 11.7). |
| `sdks/browser/src/types/options.ts:26 VisibilityConfirmationOptions` | type | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/types/options.ts:29 Credential` | type | `Credential` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/types/options.ts:38 AuthCallback` | type | `AuthCallback` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/types/options.ts:40 NetworkOptions` | type | `NetworkOptions` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/types/options.ts:54 DeviceSyncOptions` | type | `DeviceSyncOptions` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/types/options.ts:61 ContentOptions` | type | `ContentOptions` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/types/options.ts:71 StorageOptions` | type | `StorageOptions` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/types/options.ts:96 OtherOptions` | type | `OtherOptions` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/types/options.ts:129 ClientOptions` | type | `ClientOptions` | static runtime | 11.4 Browser | Host options wrapper accepts codecs or callbacks (11.1, 20.1). |
| `sdks/browser/src/types/options.ts:135 EnrichedReply` | type | `EnrichedReply` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/types/options.ts:142 BuiltInContentTypes` | type | `BuiltInContentTypes` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/types/options.ts:157 ExtractCodecContentTypes` | type | `ExtractCodecContentTypes` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/types/options.ts:167 DistributiveOmit` | type | `DistributiveOmit` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:39 encodeActions` | const | `encodeActions` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:40 encodeAttachment` | const | `encodeAttachment` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:41 encodeIntent` | const | `encodeIntent` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:42 encodeMarkdown` | const | `encodeMarkdown` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:43 encodeMultiRemoteAttachment` | const | `encodeMultiRemoteAttachment` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:46 encodeReaction` | const | `encodeReaction` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:47 encodeReadReceipt` | const | `encodeReadReceipt` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:48 encodeRemoteAttachment` | const | `encodeRemoteAttachment` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:49 encodeText` | const | `encodeText` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:50 encodeTransactionReference` | const | `encodeTransactionReference` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:51 encodeWalletSendCalls` | const | `encodeWalletSendCalls` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:54 contentTypeActions` | const | `contentTypeActions` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:55 contentTypeAttachment` | const | `contentTypeAttachment` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:56 contentTypeGroupUpdated` | const | `contentTypeGroupUpdated` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:57 contentTypeIntent` | const | `contentTypeIntent` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:58 contentTypeLeaveRequest` | const | `contentTypeLeaveRequest` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:59 contentTypeMarkdown` | const | `contentTypeMarkdown` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:60 contentTypeMultiRemoteAttachment` | const | `contentTypeMultiRemoteAttachment` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:63 contentTypeReaction` | const | `contentTypeReaction` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:64 contentTypeReadReceipt` | const | `contentTypeReadReceipt` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:65 contentTypeRemoteAttachment` | const | `contentTypeRemoteAttachment` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:68 contentTypeReply` | const | `contentTypeReply` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:69 contentTypeText` | const | `contentTypeText` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:70 contentTypeTransactionReference` | const | `contentTypeTransactionReference` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:73 contentTypeWalletSendCalls` | const | `contentTypeWalletSendCalls` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/contentTypes.ts:76 encryptAttachment` | const | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/contentTypes.ts:77 decryptAttachment` | const | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/conversions.ts:10 SafeConversation` | type | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/conversions.ts:27 toSafeConversation` | const | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/conversions.ts:45 HmacKeys` | type | — | approved removal | 11.4 Browser | Browser transport or Safe* type is replaced (11.4 Browser, 19.39). |
| `sdks/browser/src/utils/conversions.ts:47 LastReadTimes` | type | — | approved removal | 11.4 Browser | Browser transport or Safe* type is replaced (11.4 Browser, 19.39). |
| `sdks/browser/src/utils/errors.ts:1 ClientNotInitializedError` | class | `ClientNotInitializedError` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:2 ClientNotInitializedError.constructor` | member | `ClientNotInitializedError.constructor` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:9 SignerUnavailableError` | class | `SignerUnavailableError` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:10 SignerUnavailableError.constructor` | member | `SignerUnavailableError.constructor` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:17 InboxReassignError` | class | `InboxReassignError` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:18 InboxReassignError.constructor` | member | `InboxReassignError.constructor` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:25 AccountAlreadyAssociatedError` | class | `AccountAlreadyAssociatedError` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:26 AccountAlreadyAssociatedError.constructor` | member | `AccountAlreadyAssociatedError.constructor` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:31 GroupNotFoundError` | class | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/errors.ts:32 GroupNotFoundError.constructor` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/errors.ts:37 StreamNotFoundError` | class | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/errors.ts:38 StreamNotFoundError.constructor` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/errors.ts:43 StreamFailedError` | class | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/errors.ts:44 StreamFailedError.constructor` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/errors.ts:50 StreamInvalidRetryAttemptsError` | class | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/errors.ts:51 StreamInvalidRetryAttemptsError.constructor` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/errors.ts:56 OpfsNotInitializedError` | class | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/errors.ts:57 OpfsNotInitializedError.constructor` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/errors.ts:62 OpfsInitializationError` | class | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/errors.ts:63 OpfsInitializationError.constructor` | member | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/errors.ts:79 ServerConfigurationError` | class | `ServerConfigurationError` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:80 ServerConfigurationError.code` | member | `ServerConfigurationError.code` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:82 ServerConfigurationError.constructor` | member | `ServerConfigurationError.constructor` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:90 ConfigurationUnavailableError` | class | `ConfigurationUnavailableError` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:91 ConfigurationUnavailableError.constructor` | member | `ConfigurationUnavailableError.constructor` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:98 ConfigurationInvalidError` | class | `ConfigurationInvalidError` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:99 ConfigurationInvalidError.constructor` | member | `ConfigurationInvalidError.constructor` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:106 BackendMismatchError` | class | `BackendMismatchError` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:107 BackendMismatchError.constructor` | member | `BackendMismatchError.constructor` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:114 ClientVersionTooOldError` | class | `ClientVersionTooOldError` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:115 ClientVersionTooOldError.constructor` | member | `ClientVersionTooOldError.constructor` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:122 AuthRequiredError` | class | `AuthRequiredError` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:123 AuthRequiredError.constructor` | member | `AuthRequiredError.constructor` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:130 ChainNotAcceptedError` | class | `ChainNotAcceptedError` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:131 ChainNotAcceptedError.constructor` | member | `ChainNotAcceptedError.constructor` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/errors.ts:165 getErrorCode` | const | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/errors.ts:181 toServerConfigurationError` | const | `toServerConfigurationError` | generated | 11.4 Browser | Generated XmtpError variant or helper (11.1, 11.4). |
| `sdks/browser/src/utils/messages.ts:18 isReaction` | const | `isReaction` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:22 isReply` | const | `isReply` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:27 isTextReply` | const | `isTextReply` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:32 isText` | const | `isText` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:35 isRemoteAttachment` | const | `isRemoteAttachment` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:41 isAttachment` | const | `isAttachment` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:47 isMultiRemoteAttachment` | const | `isMultiRemoteAttachment` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:53 isTransactionReference` | const | `isTransactionReference` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:59 isGroupUpdated` | const | `isGroupUpdated` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:65 isReadReceipt` | const | `isReadReceipt` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:71 isLeaveRequest` | const | `isLeaveRequest` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:77 isWalletSendCalls` | const | `isWalletSendCalls` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:83 isIntent` | const | `isIntent` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:87 isActions` | const | `isActions` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/messages.ts:91 isMarkdown` | const | `isMarkdown` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/signer.ts:5 SignMessage` | type | `SignMessage` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/signer.ts:6 GetIdentifier` | type | `GetIdentifier` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/signer.ts:7 GetChainId` | type | `GetChainID` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/signer.ts:8 GetBlockNumber` | type | `GetBlockNumber` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/signer.ts:10 Signer` | type | `Signer` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/signer.ts:24 EOASigner` | type | `EOASigner` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/signer.ts:25 SCWSigner` | type | `SCWSigner` | static runtime | 11.4 Browser | Host class, codec, stream, or option type (2, 11.7). |
| `sdks/browser/src/utils/signer.ts:27 SafeSigner` | type | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/signer.ts:41 createEOASigner` | const | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/signer.ts:56 createSCWSigner` | const | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/signer.ts:75 toSafeSigner` | const | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/streamFailure.ts:4 StreamFailureCause` | type | `StreamFailureCause` | generated | 11.4 Browser | Typed stream failure details (5, 11.4). |
| `sdks/browser/src/utils/streamFailure.ts:18 UnfinishedStreamTopic` | type | `UnfinishedStreamTopic` | generated | 11.4 Browser | Typed stream failure details (5, 11.4). |
| `sdks/browser/src/utils/streamFailure.ts:29 StreamBarrierFailure` | type | `StreamBarrierFailure` | generated | 11.4 Browser | Typed stream failure details (5, 11.4). |
| `sdks/browser/src/utils/streamFailure.ts:34 StreamFailureDetails` | type | `StreamFailureDetails` | generated | 11.4 Browser | Typed stream failure details (5, 11.4). |
| `sdks/browser/src/utils/streamFailure.ts:55 getStreamFailureDetails` | const | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/streams.ts:34 DEFAULT_RETRY_DELAY` | const | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/streams.ts:35 DEFAULT_RETRY_ATTEMPTS` | const | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |
| `sdks/browser/src/utils/streams.ts:42 StreamOptions` | type | `StreamOptions` | static runtime | 11.4 Browser | Host stream adapter and options (5, 11.4). |
| `sdks/browser/src/utils/streams.ts:93 MessageStreamOptions` | type | `MessageStreamOptions` | static runtime | 11.4 Browser | Host stream adapter and options (5, 11.4). |
| `sdks/browser/src/utils/streams.ts:98 StreamCallback` | type | `StreamCallback` | static runtime | 11.4 Browser | Host stream adapter and options (5, 11.4). |
| `sdks/browser/src/utils/streams.ts:103 StreamFunction` | type | `StreamFunction` | static runtime | 11.4 Browser | Host stream adapter and options (5, 11.4). |
| `sdks/browser/src/utils/streams.ts:108 StreamValueMutator` | type | `StreamValueMutator` | static runtime | 11.4 Browser | Host stream adapter and options (5, 11.4). |
| `sdks/browser/src/utils/streams.ts:129 createStream` | const | — | approved removal | 11.4 Browser | Removal or replacement approved in 11.4 and 19. |

## Open items

2 exports need a design decision. Their proposed status appears in the SDK table.

- Swift `sdks/ios/Sources/XMTPiOS/Extensions/String.swift:5 String.hexToData`: proposed **static runtime**. The design does not name this utility export.
- Browser `sdks/browser/src/index.ts:28 metadataFieldName`: proposed **static runtime**. The design does not name this utility export.
