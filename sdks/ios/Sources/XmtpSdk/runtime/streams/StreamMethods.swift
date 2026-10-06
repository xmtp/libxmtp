import Foundation

public struct ConversationStreamOptions: Sendable {
    public var conversationKind: ConversationKind?
    public var consentStates: [ConsentState]?
    public var onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)?
    public var onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)?

    public init(
        conversationKind: ConversationKind? = nil,
        consentStates: [ConsentState]? = nil,
        onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)? = nil,
        onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)? = nil
    ) {
        self.conversationKind = conversationKind
        self.consentStates = consentStates
        self.onClose = onClose
        self.onConnectionStateChange = onConnectionStateChange
    }
}

public struct MessageStreamOptions: Sendable {
    public var conversationKind: ConversationKind?
    public var consentStates: [ConsentState]?
    public var from: String?
    public var onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)?
    public var onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)?

    public init(
        conversationKind: ConversationKind? = nil,
        consentStates: [ConsentState]? = nil,
        from: String? = nil,
        onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)? = nil,
        onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)? = nil
    ) {
        self.conversationKind = conversationKind
        self.consentStates = consentStates
        self.from = from
        self.onClose = onClose
        self.onConnectionStateChange = onConnectionStateChange
    }
}

public struct ConversationMessageStreamOptions: Sendable {
    public var from: String?
    public var onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)?
    public var onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)?

    public init(
        from: String? = nil,
        onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)? = nil,
        onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)? = nil
    ) {
        self.from = from
        self.onClose = onClose
        self.onConnectionStateChange = onConnectionStateChange
    }
}

private func streamOwner(_ key: UInt64) throws -> SDKClient {
    guard let owner = ClientRegistry.get(key) else { throw clientClosedError() }
    return owner
}

public extension Conversations {
    /// The sequence holds its client only after a reader opens.
    func stream(options: ConversationStreamOptions = .init()) async throws -> SDKConversationStream {
        try Task.checkCancellation()
        let owner = try streamOwner(sdkStreamOwnerKey())
        return makeSDKConversationStream(kind: options.conversationKind, consentStates: options.consentStates,
                                         owner: owner, onClose: options.onClose, onConnectionStateChange: options.onConnectionStateChange)
    }

    /// The next read acknowledges the previous message. The active reader holds its client.
    func streamAllMessages(options: MessageStreamOptions = .init()) async throws -> SDKMessageStream {
        try Task.checkCancellation()
        let owner = try streamOwner(sdkStreamOwnerKey())
        let selection = MessageReaderOptions(conversationKind: options.conversationKind, consentStates: options.consentStates, from: options.from)
        return makeSDKMessageStream(open: { try await self.messageReader(options: selection) }, owner: owner,
                                    onClose: options.onClose, onConnectionStateChange: options.onConnectionStateChange)
    }
}

public extension Group {
    /// The next read acknowledges the previous message. The active reader holds its client.
    func streamMessages(options: ConversationMessageStreamOptions = .init()) async throws -> SDKMessageStream {
        try Task.checkCancellation()
        let owner = try streamOwner(sdkStreamOwnerKey())
        let selection = ConversationMessageReaderOptions(from: options.from)
        return makeSDKMessageStream(open: { try await self.messageReader(options: selection) }, owner: owner,
                                    onClose: options.onClose, onConnectionStateChange: options.onConnectionStateChange)
    }
}

public extension Dm {
    /// The next read acknowledges the previous message. The active reader holds its client.
    func streamMessages(options: ConversationMessageStreamOptions = .init()) async throws -> SDKMessageStream {
        try Task.checkCancellation()
        let owner = try streamOwner(sdkStreamOwnerKey())
        let selection = ConversationMessageReaderOptions(from: options.from)
        return makeSDKMessageStream(open: { try await self.messageReader(options: selection) }, owner: owner,
                                    onClose: options.onClose, onConnectionStateChange: options.onConnectionStateChange)
    }
}

public extension Conversation {
    /// The next read acknowledges the previous message. The active reader holds its client.
    func streamMessages(options: ConversationMessageStreamOptions = .init()) async throws -> SDKMessageStream {
        switch self {
        case let .group(group): return try await group.streamMessages(options: options)
        case let .dm(dm): return try await dm.streamMessages(options: options)
        }
    }
}
