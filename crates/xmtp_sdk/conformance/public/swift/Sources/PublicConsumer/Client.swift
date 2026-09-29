import Foundation
import XmtpSdk

/// This list comes from the retained Client surface in the approved plan, not
/// from the generator. Removing a public forwarder must fail this compile.
public func consumeClient(
    _ signer: Signer, _ options: ClientOptions, _ identity: PublicIdentity,
    _ credential: Credential, _ request: SignatureRequest, _ filter: EventFilter,
    _ config: NotificationConfig
) async throws {
    let client = try await SDKClient.create(signer: signer, options: options)
    let built = try await SDKClient.build(identity: identity, options: options, inboxId: client.inboxId())
    let _: InboxId = client.inboxId()
    let _: InstallationId = client.installationId()
    let _: Data = client.installationIdBytes()
    let _: PublicIdentity = client.identity()
    let _: Bool = client.isInMemory()
    let _: String? = client.storagePath()
    let _: String = client.libxmtpVersion()
    let _: String? = client.appVersion()
    let _: ClientOptions = client.options()
    let _: Conversations = client.conversations()
    let _: Preferences = client.preferences()
    let _: Diagnostics = client.diagnostics()
    let _: Archives = client.archives()
    let _: Storage = client.storage()
    try await client.register()
    let _: Bool = try await client.isRegistered()
    let _: InboxState = try await client.inboxState(refreshFromNetwork: true)
    let _: [InboxState] = try await client.inboxStates(ids: [client.inboxId()], refreshFromNetwork: false)
    let _: InboxId? = try await client.inboxIdFor(identity: identity)
    let _: [String: Bool] = try await client.canMessage(identities: [identity])
    let _: [String: UInt64] = try await client.latestInboxUpdatesCount(ids: [client.inboxId()], refreshFromNetwork: false)
    let _: UInt64 = try await client.ownInboxUpdatesCount(refreshFromNetwork: false)
    let _: [String: KeyPackageStatus] = try await client.keyPackageStatuses(ids: [client.installationId()])
    let _: ServerConfiguration = client.serverConfiguration()
    let _: ServerConfiguration = try await client.refreshServerConfiguration()
    try await client.setCredential(credential: credential)
    let _: Data = try await client.signWithInstallationKey(text: "text")
    let _: Bool = try await client.verifySignedWithInstallationKey(text: "text", signature: Data())
    let _: CatchUpSummary = try await client.catchUpToLive(timeoutMs: nil)
    let _: GroupSyncSummary = try await client.syncAllDeviceSyncGroups()
    let _: MessageContent = try await client.decodeContent(encoded: encodeText(text: "text"))
    let _: SignatureRequest? = try await client.unsafeCreateInboxSignatureRequest()
    let _: SignatureRequest = try await client.unsafeAddAccountSignatureRequest(identity: identity, allowInboxReassign: false)
    let _: SignatureRequest = try await client.unsafeRemoveAccountSignatureRequest(identity: identity)
    let _: SignatureRequest = try await client.unsafeRevokeInstallationsSignatureRequest(ids: [])
    let _: SignatureRequest? = try await client.unsafeRevokeAllOtherInstallationsSignatureRequest()
    let _: SignatureRequest = try await client.unsafeChangeRecoveryIdentifierSignatureRequest(identity: identity)
    try await client.unsafeApplySignatureRequest(request: request)
    try await client.unsafeAddAccount(signer: signer, allowInboxReassign: false)
    try await client.removeAccount(recoverySigner: signer, identity: identity)
    try await client.revokeInstallations(signer: signer, ids: [])
    try await client.revokeAllOtherInstallations(signer: signer)
    try await client.changeRecoveryIdentifier(signer: signer, identity: identity)
    let _: NotificationState = try await client.enableNotifications(config: config)
    let _: NotificationState = try client.notificationState()
    try await client.disableNotifications()
    let events: SDKEventStream = try await client.events(filter)
    _ = events
    let listener = try await client.startListener(filter) { _ in }
    await client.stopListener(listener)
    try await built.end()
    try await client.end()
}

public func consumeStaticHelpers(_ signer: Signer, _ identity: PublicIdentity, _ backend: BackendSource) async throws {
    let _: ServerConfiguration = try await SDKClient.fetchServerConfiguration(backend: backend)
    let _: [String: Bool] = try await SDKClient.canMessage([identity], backend: backend)
    let inbox: InboxId = try await SDKClient.inboxId(for: identity, backend: backend)
    let _: [InboxState] = try await SDKClient.inboxStates([inbox], backend: backend)
    let _: [String: KeyPackageStatus] = try await SDKClient.keyPackageStatuses([], backend: backend)
    let _: [String: MessageMetadataEntry] = try await SDKClient.newestMessageMetadata([], backend: backend)
    try await SDKClient.revokeInstallations(signer: signer, inboxId: inbox, ids: [], backend: backend)
    let _: Bool = try await SDKClient.isAddressAuthorized("address", inboxId: inbox, backend: backend)
    let _: Bool = try await SDKClient.isInstallationAuthorized("installation", inboxId: inbox, backend: backend)
    let _: Bool = try await SDKClient.verifySignedWithPublicKey("text", signature: Data(), publicKey: Data())
}
