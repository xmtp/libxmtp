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

// These typed adapters own selection and host lifetime policy. The generator
// supplies only the declared owner getter and reader opener.
private func openConversationStreamOptions(
    ownerKey: () -> UInt64,
    open: @escaping @Sendable (ConversationReaderOptions?) async throws -> ConversationReader,
    options: ConversationStreamOptions
) async throws -> SDKConversationStream {
    try Task.checkCancellation()
    let owner = try streamOwner(ownerKey())
    let selection = ConversationReaderOptions(kind: options.conversationKind, consentStates: options.consentStates)
    return makeSDKConversationStream(open: { try await open(selection) }, owner: owner,
                                     onClose: options.onClose, onConnectionStateChange: options.onConnectionStateChange)
}

private func openMessageStreamOptions(
    ownerKey: () -> UInt64,
    open: @escaping @Sendable (MessageReaderOptions?) async throws -> MessageReader,
    options: MessageStreamOptions
) async throws -> SDKMessageStream {
    try Task.checkCancellation()
    let owner = try streamOwner(ownerKey())
    let selection = MessageReaderOptions(conversationKind: options.conversationKind, consentStates: options.consentStates, from: options.from)
    return makeSDKMessageStream(open: { try await open(selection) }, owner: owner,
                                onClose: options.onClose, onConnectionStateChange: options.onConnectionStateChange)
}

private func openConversationMessageStreamOptions(
    ownerKey: () -> UInt64,
    open: @escaping @Sendable (ConversationMessageReaderOptions?) async throws -> MessageReader,
    options: ConversationMessageStreamOptions
) async throws -> SDKMessageStream {
    try Task.checkCancellation()
    let owner = try streamOwner(ownerKey())
    let selection = ConversationMessageReaderOptions(from: options.from)
    return makeSDKMessageStream(open: { try await open(selection) }, owner: owner,
                                onClose: options.onClose, onConnectionStateChange: options.onConnectionStateChange)
}
