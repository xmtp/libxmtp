import Foundation

/// The host client resolves storage and owns the weak message lookup entry.
public final class SDKClient: @unchecked Sendable {
    public let raw: Client

    private init(_ raw: Client) {
        self.raw = raw
        ClientRegistry.register(raw)
    }

    private static func resolved(_ options: ClientOptions) -> ClientOptions {
        var result = options
        if case .default = result.storage.location {
            let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
                .first ?? FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            result.storage.location = .directory(base.appendingPathComponent("xmtp-sdk").path)
        }
        return result
    }

    public static func create(signer: Signer, options: ClientOptions) async throws -> SDKClient {
        try await SDKClient(Client.create(signer: signer, options: resolved(options)))
    }

    public static func build(
        identity: PublicIdentity, options: ClientOptions, inboxID: InboxID? = nil
    ) async throws -> SDKClient {
        try await SDKClient(Client.build(identity: identity, options: resolved(options), inboxID: inboxID))
    }

    public func end() async throws {
        defer { ClientRegistry.remove(raw) }
        try await raw.end()
    }

    /// The reader acknowledges a value when the next read starts.
    public func messages(in group: Group) async throws -> AsyncThrowingStream<Message, Error> {
        let reader = try await group.messageReader()
        if Task.isCancelled {
            try? await reader.end()
            throw CancellationError()
        }
        let owner = self
        return AsyncThrowingStream(unfolding: {
            try await withTaskCancellationHandler {
                _ = owner.raw
                let value = try await reader.next()
                if value == nil {
                    try await reader.end()
                }
                return value
            } onCancel: {
                Task { try? await reader.end() }
            }
        })
    }
}
