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

    private static func resolved(_ options: ClientOptions, appName: String?) -> ClientOptions {
        var result = options
        if case .default = result.storage.location {
            let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
                .first ?? FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            let name = appName ?? Bundle.main.bundleIdentifier ?? "xmtp-sdk"
            result.storage.location = .directory(base.appendingPathComponent(name)
                .appendingPathComponent("xmtp").path)
        }
        return result
    }

    public static func create(
        signer: Signer, options: ClientOptions, appName: String? = nil,
        codecs: [any SDKContentCodec] = []
    ) async throws -> SDKClient {
        try await SDKClient(Client.create(signer: signer, options: resolved(options, appName: appName)), codecs: codecs)
    }

    public static func build(
        identity: PublicIdentity, options: ClientOptions, inboxID: InboxID? = nil,
        appName: String? = nil, codecs: [any SDKContentCodec] = []
    ) async throws -> SDKClient {
        try await SDKClient(Client.build(identity: identity, options: resolved(options, appName: appName), inboxID: inboxID), codecs: codecs)
    }

    public static func fetchServerConfiguration(backend: BackendSource) async throws -> ServerConfiguration {
        try await XmtpSdk.fetchServerConfiguration(backend: backend)
    }

    public static func canMessage(_ identities: [PublicIdentity], backend: BackendSource) async throws -> [CanMessageEntry] {
        try await canMessageWithBackend(backend: backend, identities: identities)
    }

    public static func inboxID(for identity: PublicIdentity, backend: BackendSource) async throws -> InboxID {
        try await inboxIDForWithBackend(backend: backend, identity: identity)
    }

    public static func inboxStates(_ ids: [InboxID], backend: BackendSource) async throws -> [InboxState] {
        try await inboxStatesWithBackend(backend: backend, ids: ids)
    }

    public static func keyPackageStatuses(_ ids: [InstallationID], backend: BackendSource) async throws -> [KeyPackageStatusEntry] {
        try await keyPackageStatusesWithBackend(backend: backend, ids: ids)
    }

    public static func newestMessageMetadata(_ ids: [ConversationID], backend: BackendSource) async throws -> [MessageMetadataEntry] {
        try await newestMessageMetadataWithBackend(backend: backend, ids: ids)
    }

    public static func revokeInstallations(signer: Signer, inboxID: InboxID, ids: [InstallationID], backend: BackendSource) async throws {
        try await revokeInstallationsWithBackend(backend: backend, signer: signer, inboxID: inboxID, ids: ids)
    }

    public static func isAddressAuthorized(_ address: String, inboxID: InboxID, backend: BackendSource) async throws -> Bool {
        try await isAddressAuthorizedWithBackend(backend: backend, inboxID: inboxID, address: address)
    }

    public static func isInstallationAuthorized(_ installationID: InstallationID, inboxID: InboxID, backend: BackendSource) async throws -> Bool {
        try await isInstallationAuthorizedWithBackend(backend: backend, inboxID: inboxID, installationID: installationID)
    }

    public static func verifySignedWithPublicKey(_ text: String, signature: Data, publicKey: Data) async throws -> Bool {
        try await XmtpSdk.verifySignedWithPublicKey(text: text, signature: signature, publicKey: publicKey)
    }

    public func end() async throws {
        defer { ClientRegistry.remove(self) }
        try await raw.end()
    }

    /// The reader acknowledges a value when the next read starts.
    public func messages(in group: Group) async throws -> SDKMessageStream {
        let reader = try await group.messageReader()
        if Task.isCancelled {
            try? await reader.end()
            throw CancellationError()
        }
        return SDKMessageStream(reader: reader, owner: self)
    }
}

/// Each request reads one value. Releasing the iterator closes the reader.
public struct SDKMessageStream: AsyncSequence {
    public typealias Element = Message
    private let reader: MessageReader
    private let owner: SDKClient

    fileprivate init(reader: MessageReader, owner: SDKClient) {
        self.reader = reader
        self.owner = owner
    }

    public func makeAsyncIterator() -> Iterator {
        Iterator(reader: reader, owner: owner)
    }

    public final class Iterator: AsyncIteratorProtocol {
        private let reader: MessageReader
        private let owner: SDKClient
        private var closed = false

        fileprivate init(reader: MessageReader, owner: SDKClient) {
            self.reader = reader
            self.owner = owner
        }

        public func next() async throws -> Message? {
            if closed {
                return nil
            }
            return try await withTaskCancellationHandler {
                _ = owner.raw
                let value = try await reader.next()
                if value == nil {
                    closed = true
                    try await reader.end()
                }
                return value
            } onCancel: {
                Task { try? await reader.end() }
            }
        }

        deinit {
            let reader = reader
            Task { try? await reader.end() }
        }
    }
}
