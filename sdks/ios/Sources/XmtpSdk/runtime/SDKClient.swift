import Foundation

/// The receive registry. It erases each codec's value type in a private decode
/// closure, the one place where the value type is erased (Decision 3).
private struct CodecRegistry {
    private typealias Decode = @Sendable (EncodedContent) throws -> any Sendable
    private let decoders: [ContentCodecKey: Decode]

    init(_ codecs: [any ContentCodec]) {
        decoders = Dictionary(
            codecs.map { (ContentCodecKey($0.type), Self.decoder($0)) },
            uniquingKeysWith: { _, newer in newer }
        )
    }

    private static func decoder<C: ContentCodec>(_ codec: C) -> Decode {
        { try codec.decode($0) }
    }

    func decode(_ encoded: EncodedContent, rawBytes: Data) -> SDKMessageContent {
        guard let decode = decoders[ContentCodecKey(encoded.type)] else { return .unknown(encoded: encoded, rawBytes: rawBytes, error: ErrorDetails(code: "CodecNotFound", category: .input, retryable: false, message: "content type has no registered host codec")) }
        do { return try .custom(encoded: encoded, rawBytes: rawBytes, value: decode(encoded), error: nil) }
        catch { return .custom(encoded: encoded, rawBytes: rawBytes, value: nil, error: ErrorDetails(code: "CodecDecodeFailed", category: .callback, retryable: false, message: String(describing: error))) }
    }
}

/// The host client resolves storage and owns the weak message lookup entry.
/// Generated forwarders in `ClientForwarding.swift` expose the other Client
/// methods. The generated Client stays private to the runtime.
public final class SDKClient: @unchecked Sendable {
    let raw: Client
    let listenerGates = ListenerGates()
    private let codecs: CodecRegistry

    private init(_ raw: Client, codecs: [any ContentCodec]) {
        self.raw = raw
        self.codecs = CodecRegistry(codecs)
        ClientRegistry.register(self)
    }

    public func storage() -> Storage {
        raw.storage()
    }

    func decodeCustom(_ encoded: EncodedContent, rawBytes: Data) -> SDKMessageContent {
        codecs.decode(encoded, rawBytes: rawBytes)
    }

    private static func resolved(_ options: ClientOptions) throws -> ClientOptions {
        var result = options
        if case .default = result.storage.location {
            guard let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first,
                  let name = Bundle.main.bundleIdentifier, !name.isEmpty, !name.contains("\0")
            else {
                throw XmtpError.StorageLocationRequired(ErrorDetails(
                    code: "StorageLocationRequired", category: .storage, retryable: false,
                    message: "Default storage needs an application bundle identifier and Application Support directory"
                ))
            }
            result.storage.location = .directory(directory: base.appendingPathComponent(name)
                .appendingPathComponent("xmtp").path)
        }
        return result
    }

    public static func create(
        signer: Signer, options: ClientOptions,
        codecs: [any ContentCodec] = []
    ) async throws -> SDKClient {
        #if canImport(UIKit)
            await AppleStreamLifecycle.enableIfNeeded()
        #endif
        return try await SDKClient(Client.create(signer: signer, options: resolved(options)), codecs: codecs)
    }

    public static func build(
        identity: PublicIdentity, options: ClientOptions, inboxId: InboxId? = nil,
        codecs: [any ContentCodec] = []
    ) async throws -> SDKClient {
        #if canImport(UIKit)
            await AppleStreamLifecycle.enableIfNeeded()
        #endif
        return try await SDKClient(Client.build(identity: identity, options: resolved(options), inboxId: inboxId), codecs: codecs)
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
        try await raw.end()
        ClientRegistry.remove(self)
    }

    /// The reader acknowledges a value when the next read starts.
    public func messages(
        in group: Group,
        options: ConversationMessageReaderOptions? = nil,
        onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)? = nil,
        onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)? = nil
    ) async throws -> SDKMessageStream {
        try Task.checkCancellation()
        return makeSDKMessageStream(
            open: { try await group.messageReader(options: options) }, owner: self, onClose: onClose,
            onConnectionStateChange: onConnectionStateChange
        )
    }

    public func messages(
        in dm: Dm,
        options: ConversationMessageReaderOptions? = nil,
        onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)? = nil,
        onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)? = nil
    ) async throws -> SDKMessageStream {
        try Task.checkCancellation()
        return makeSDKMessageStream(
            open: { try await dm.messageReader(options: options) }, owner: self, onClose: onClose,
            onConnectionStateChange: onConnectionStateChange
        )
    }

    public func messages(
        options: MessageReaderOptions? = nil,
        onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)? = nil,
        onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)? = nil
    ) async throws -> SDKMessageStream {
        try Task.checkCancellation()
        let conversations = raw.conversations()
        return makeSDKMessageStream(
            open: { try await conversations.messageReader(options: options) }, owner: self, onClose: onClose,
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
