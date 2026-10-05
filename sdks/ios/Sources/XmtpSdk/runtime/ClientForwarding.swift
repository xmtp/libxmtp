/// Generated from exported Client methods. Do not edit this output.
import Foundation

public extension SDKClient {
    func appVersion() -> String? {
        raw.appVersion()
    }

    func archives() -> Archives {
        raw.archives()
    }

    func attachments() -> Attachments {
        raw.attachments()
    }

    func canMessage(identities: [PublicIdentity]) async throws -> [String: Bool] {
        try await raw.canMessage(identities: identities)
    }

    func catchUpToLive(timeoutMs: UInt64?) async throws -> CatchUpSummary {
        try await raw.catchUpToLive(timeoutMs: timeoutMs)
    }

    func changeRecoveryIdentifier(signer: Signer, identity: PublicIdentity) async throws {
        try await raw.changeRecoveryIdentifier(signer: signer, identity: identity)
    }

    func conversations() -> Conversations {
        raw.conversations()
    }

    func decodeContent(encoded: EncodedContent) async throws -> MessageContent {
        try await raw.decodeContent(encoded: encoded)
    }

    func diagnostics() -> Diagnostics {
        raw.diagnostics()
    }

    func disableNotifications() async throws {
        try await raw.disableNotifications()
    }

    func enableNotifications(config: NotificationConfig) async throws -> NotificationState {
        try await raw.enableNotifications(config: config)
    }

    func identity() -> PublicIdentity {
        raw.identity()
    }

    func inboxId() -> InboxId {
        raw.inboxId()
    }

    func inboxIdFor(identity: PublicIdentity) async throws -> InboxId? {
        try await raw.inboxIdFor(identity: identity)
    }

    func inboxState(refreshFromNetwork: Bool) async throws -> InboxState {
        try await raw.inboxState(refreshFromNetwork: refreshFromNetwork)
    }

    func inboxStates(ids: [InboxId], refreshFromNetwork: Bool) async throws -> [InboxState] {
        try await raw.inboxStates(ids: ids, refreshFromNetwork: refreshFromNetwork)
    }

    func installationId() -> InstallationId {
        raw.installationId()
    }

    func installationIdBytes() -> Data {
        raw.installationIdBytes()
    }

    func isInMemory() -> Bool {
        raw.isInMemory()
    }

    func isRegistered() async throws -> Bool {
        try await raw.isRegistered()
    }

    func keyPackageStatuses(ids: [InstallationId]) async throws -> [String: KeyPackageStatus] {
        try await raw.keyPackageStatuses(ids: ids)
    }

    func latestInboxUpdatesCount(ids: [InboxId], refreshFromNetwork: Bool) async throws -> [String: UInt64] {
        try await raw.latestInboxUpdatesCount(ids: ids, refreshFromNetwork: refreshFromNetwork)
    }

    func libxmtpVersion() -> String {
        raw.libxmtpVersion()
    }

    func notificationState() throws -> NotificationState {
        try raw.notificationState()
    }

    func options() -> ClientOptions {
        raw.options()
    }

    func ownInboxUpdatesCount(refreshFromNetwork: Bool) async throws -> UInt64 {
        try await raw.ownInboxUpdatesCount(refreshFromNetwork: refreshFromNetwork)
    }

    func preferences() -> Preferences {
        raw.preferences()
    }

    func refreshServerConfiguration() async throws -> ServerConfiguration {
        try await raw.refreshServerConfiguration()
    }

    func register() async throws {
        try await raw.register()
    }

    func removeAccount(recoverySigner: Signer, identity: PublicIdentity) async throws {
        try await raw.removeAccount(recoverySigner: recoverySigner, identity: identity)
    }

    func revokeAllOtherInstallations(signer: Signer) async throws {
        try await raw.revokeAllOtherInstallations(signer: signer)
    }

    func revokeInstallations(signer: Signer, ids: [InstallationId]) async throws {
        try await raw.revokeInstallations(signer: signer, ids: ids)
    }

    func serverConfiguration() -> ServerConfiguration {
        raw.serverConfiguration()
    }

    func setCredential(credential: Credential) async throws {
        try await raw.setCredential(credential: credential)
    }

    func signWithInstallationKey(text: String) async throws -> Data {
        try await raw.signWithInstallationKey(text: text)
    }

    func storagePath() -> String? {
        raw.storagePath()
    }

    func syncAllDeviceSyncGroups() async throws -> GroupSyncSummary {
        try await raw.syncAllDeviceSyncGroups()
    }

    func unsafeAddAccount(signer: Signer, allowInboxReassign: Bool) async throws {
        try await raw.unsafeAddAccount(signer: signer, allowInboxReassign: allowInboxReassign)
    }

    func unsafeAddAccountSignatureRequest(identity: PublicIdentity, allowInboxReassign: Bool) async throws -> SignatureRequest {
        try await raw.unsafeAddAccountSignatureRequest(identity: identity, allowInboxReassign: allowInboxReassign)
    }

    func unsafeApplySignatureRequest(request: SignatureRequest) async throws {
        try await raw.unsafeApplySignatureRequest(request: request)
    }

    func unsafeChangeRecoveryIdentifierSignatureRequest(identity: PublicIdentity) async throws -> SignatureRequest {
        try await raw.unsafeChangeRecoveryIdentifierSignatureRequest(identity: identity)
    }

    func unsafeCreateInboxSignatureRequest() async throws -> SignatureRequest? {
        try await raw.unsafeCreateInboxSignatureRequest()
    }

    func unsafeRemoveAccountSignatureRequest(identity: PublicIdentity) async throws -> SignatureRequest {
        try await raw.unsafeRemoveAccountSignatureRequest(identity: identity)
    }

    func unsafeRevokeAllOtherInstallationsSignatureRequest() async throws -> SignatureRequest? {
        try await raw.unsafeRevokeAllOtherInstallationsSignatureRequest()
    }

    func unsafeRevokeInstallationsSignatureRequest(ids: [InstallationId]) async throws -> SignatureRequest {
        try await raw.unsafeRevokeInstallationsSignatureRequest(ids: ids)
    }

    func verifySignedWithInstallationKey(text: String, signature: Data) async throws -> Bool {
        try await raw.verifySignedWithInstallationKey(text: text, signature: signature)
    }
}
