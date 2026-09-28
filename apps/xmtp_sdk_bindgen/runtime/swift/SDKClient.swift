import Foundation

private struct CodecRegistry {
    private let codecs: [SDKContentCodecKey: any SDKContentCodec]

    init(_ codecs: [any SDKContentCodec]) {
        self.codecs = Dictionary(codecs.map { ($0.key, $0) }, uniquingKeysWith: { _, newer in newer })
    }

    func decode(_ encoded: EncodedContent) -> SDKMessageContent {
        guard let codec = codecs[SDKContentCodecKey(encoded.type)] else { return .unknown(encoded) }
        do { return try .custom(encoded: encoded, value: codec.decode(encoded), error: nil) }
        catch { return .custom(encoded: encoded, value: nil, error: error) }
    }
}

/// The host client resolves storage and owns the weak message lookup entry.
public final class SDKClient: @unchecked Sendable {
    public let raw: Client
    let listenerGates = ListenerGates()
    private let codecs: CodecRegistry

    private init(_ raw: Client, codecs: [any SDKContentCodec]) {
        self.raw = raw
        self.codecs = CodecRegistry(codecs)
        ClientRegistry.register(self)
    }

    public func storage() -> Storage {
        raw.storage()
    }

    func decodeCustom(_ encoded: EncodedContent) -> SDKMessageContent {
        codecs.decode(encoded)
    }

    private static func resolved(_ options: ClientOptions) throws -> ClientOptions {
        var result = options
        if case .default = result.storage.location {
            guard let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first,
                  let name = Bundle.main.bundleIdentifier, !name.isEmpty, !name.contains("\0") else {
                throw XmtpError.StorageLocationRequired(ErrorDetails(
                    code: "StorageLocationRequired", category: .storage, retryable: false,
                    message: "Default storage needs an application bundle identifier and Application Support directory"))
            }
            result.storage.location = .directory(base.appendingPathComponent(name)
                .appendingPathComponent("xmtp").path)
        }
        return result
    }

    public static func create(
        signer: Signer, options: ClientOptions,
        codecs: [any SDKContentCodec] = []
    ) async throws -> SDKClient {
        try await SDKClient(Client.create(signer: signer, options: try resolved(options)), codecs: codecs)
    }

    public static func build(
        identity: PublicIdentity, options: ClientOptions, inboxId: InboxId? = nil,
        codecs: [any SDKContentCodec] = []
    ) async throws -> SDKClient {
        try await SDKClient(Client.build(identity: identity, options: try resolved(options), inboxId: inboxId), codecs: codecs)
    }

    public static func fetchServerConfiguration(backend: BackendSource) async throws -> ServerConfiguration {
        try await XmtpSdk.fetchServerConfiguration(backend: backend)
    }

    public static func canMessage(_ identities: [PublicIdentity], backend: BackendSource) async throws -> [String: Bool] {
        try await canMessageWithBackend(backend: backend, identities: identities)
    }

    public static func inboxId(for identity: PublicIdentity, backend: BackendSource) async throws -> InboxId {
        try await inboxIdForWithBackend(backend: backend, identity: identity)
    }

    public static func inboxStates(_ ids: [InboxId], backend: BackendSource) async throws -> [InboxState] {
        try await inboxStatesWithBackend(backend: backend, ids: ids)
    }

    public static func keyPackageStatuses(_ ids: [InstallationId], backend: BackendSource) async throws -> [String: KeyPackageStatus] {
        try await keyPackageStatusesWithBackend(backend: backend, ids: ids)
    }

    public static func newestMessageMetadata(_ ids: [ConversationId], backend: BackendSource) async throws -> [String: MessageMetadataEntry] {
        try await newestMessageMetadataWithBackend(backend: backend, ids: ids)
    }

    public static func revokeInstallations(signer: Signer, inboxId: InboxId, ids: [InstallationId], backend: BackendSource) async throws {
        try await revokeInstallationsWithBackend(backend: backend, signer: signer, inboxId: inboxId, ids: ids)
    }

    public static func isAddressAuthorized(_ address: String, inboxId: InboxId, backend: BackendSource) async throws -> Bool {
        try await isAddressAuthorizedWithBackend(backend: backend, inboxId: inboxId, address: address)
    }

    public static func isInstallationAuthorized(_ installationId: InstallationId, inboxId: InboxId, backend: BackendSource) async throws -> Bool {
        try await isInstallationAuthorizedWithBackend(backend: backend, inboxId: inboxId, installationId: installationId)
    }

    public static func verifySignedWithPublicKey(_ text: String, signature: Data, publicKey: Data) async throws -> Bool {
        try await XmtpSdk.verifySignedWithPublicKey(text: text, signature: signature, publicKey: publicKey)
    }

    public func end() async throws {
        listenerGates.stopAll()
        defer { ClientRegistry.remove(self) }
        try await raw.end()
    }

    /// The reader acknowledges a value when the next read starts.
    public func messages(
        in group: Group,
        onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)? = nil,
        onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)? = nil
    ) async throws -> SDKMessageStream {
        try Task.checkCancellation()
        return makeSDKMessageStream(
            group: group, owner: self, onClose: onClose,
            onConnectionStateChange: onConnectionStateChange
        )
    }

    public func conversationStream(
        kind: ConversationKind? = nil,
        consentStates: [ConsentState]? = nil,
        onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)? = nil,
        onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)? = nil
    ) async throws -> SDKConversationStream {
        try Task.checkCancellation()
        return makeSDKConversationStream(
            kind: kind, consentStates: consentStates, owner: self, onClose: onClose,
            onConnectionStateChange: onConnectionStateChange
        )
    }
}
