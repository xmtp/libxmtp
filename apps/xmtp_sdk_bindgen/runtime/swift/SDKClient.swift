import Foundation

/// The host client resolves storage and owns the weak message lookup entry.
public final class SDKClient: @unchecked Sendable {
    public let raw: Client

    private init(_ raw: Client) {
        self.raw = raw
        ClientRegistry.register(self)
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
        signer: Signer, options: ClientOptions, appName: String? = nil
    ) async throws -> SDKClient {
        try await SDKClient(Client.create(signer: signer, options: resolved(options, appName: appName)))
    }

    public static func build(
        identity: PublicIdentity, options: ClientOptions, inboxID: InboxID? = nil,
        appName: String? = nil
    ) async throws -> SDKClient {
        try await SDKClient(Client.build(identity: identity, options: resolved(options, appName: appName), inboxID: inboxID))
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
