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
/// methods and the Client statics. The generated Client stays private to the
/// runtime.
public final class SDKClient: @unchecked Sendable {
    let raw: Client
    let listenerGates = ListenerGates()
    private let codecs: CodecRegistry

    /// Internal, not private, so tests can wrap a Client fake.
    init(_ raw: Client, codecs: [any ContentCodec]) {
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

    public func end() async throws {
        listenerGates.stopAll()
        try await raw.end()
        ClientRegistry.remove(self)
    }

    public var conversations: Conversations {
        raw.conversations()
    }
}
