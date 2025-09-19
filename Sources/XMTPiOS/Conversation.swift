import Foundation
import LibXMTP

public enum Conversation: Identifiable, Equatable, Hashable {
	case group(Group)
	case dm(Dm)

	public static func == (lhs: Conversation, rhs: Conversation) -> Bool {
		lhs.topic == rhs.topic
	}

	public func hash(into hasher: inout Hasher) {
		hasher.combine(topic)
	}

	public enum XMTPConversationType {
		case group, dm
	}

	public var id: String {
		switch self {
		case let .group(group):
			return group.id
		case let .dm(dm):
			return dm.id
		}
	}

	public var disappearingMessageSettings: DisappearingMessageSettings? {
		switch self {
		case let .group(group):
			return group.disappearingMessageSettings
		case let .dm(dm):
			return dm.disappearingMessageSettings
		}
	}

	public func isDisappearingMessagesEnabled() throws -> Bool {
		switch self {
		case let .group(group):
			return try group.isDisappearingMessagesEnabled()
		case let .dm(dm):
			return try dm.isDisappearingMessagesEnabled()
		}
	}

	public func lastMessage() async throws -> DecodedMessage? {
		switch self {
		case let .group(group):
			return try await group.lastMessage()
		case let .dm(dm):
			return try await dm.lastMessage()
		}
	}

    public func commitLogForkStatus() -> CommitLogForkStatus {
        switch self {
        case let .group(group):
            return group.commitLogForkStatus()
        case let .dm(dm):
            return dm.commitLogForkStatus()
        }
    }

	public func isCreator() async throws -> Bool {
		switch self {
		case let .group(group):
			return try await group.isCreator()
		case let .dm(dm):
			return try await dm.isCreator()
		}
	}

	public func members() async throws -> [Member] {
		switch self {
		case let .group(group):
			return try await group.members
		case let .dm(dm):
			return try await dm.members
		}
	}

	public func consentState() throws -> ConsentState {
		switch self {
		case let .group(group):
			return try group.consentState()
		case let .dm(dm):
			return try dm.consentState()
		}
	}

	public func updateConsentState(state: ConsentState) async throws {
		switch self {
		case let .group(group):
			try await group.updateConsentState(state: state)
		case let .dm(dm):
			try await dm.updateConsentState(state: state)
		}
	}

	public func updateDisappearingMessageSettings(
		_ disappearingMessageSettings: DisappearingMessageSettings?
	) async throws {
		switch self {
		case let .group(group):
			try await group.updateDisappearingMessageSettings(
				disappearingMessageSettings)
		case let .dm(dm):
			try await dm.updateDisappearingMessageSettings(
				disappearingMessageSettings)
		}
	}

	public func clearDisappearingMessageSettings() async throws {
		switch self {
		case let .group(group):
			try await group.clearDisappearingMessageSettings()
		case let .dm(dm):
			try await dm.clearDisappearingMessageSettings()
		}
	}

	public func sync() async throws {
		switch self {
		case let .group(group):
			try await group.sync()
		case let .dm(dm):
			try await dm.sync()
		}
	}

	public func processMessage(messageBytes: Data) async throws -> DecodedMessage? {
		switch self {
		case let .group(group):
			return try await group.processMessage(messageBytes: messageBytes)
		case let .dm(dm):
			return try await dm.processMessage(messageBytes: messageBytes)
		}
	}

	public func prepareMessage(encodedContent: EncodedContent) async throws
		-> String
	{
		switch self {
		case let .group(group):
			return try await group.prepareMessage(
				encodedContent: encodedContent)
		case let .dm(dm):
			return try await dm.prepareMessage(encodedContent: encodedContent)
		}
	}

	public func prepareMessage<T>(content: T, options: SendOptions? = nil)
		async throws -> String
	{
		switch self {
		case let .group(group):
			return try await group.prepareMessage(
				content: content, options: options)
		case let .dm(dm):
			return try await dm.prepareMessage(
				content: content, options: options)
		}
	}

	public func publishMessages() async throws {
		switch self {
		case let .group(group):
			return try await group.publishMessages()
		case let .dm(dm):
			return try await dm.publishMessages()
		}
	}

	public var type: XMTPConversationType {
		switch self {
		case .group:
			return .group
		case .dm:
			return .dm
		}
	}

	public var createdAt: Date {
		switch self {
		case let .group(group):
			return group.createdAt
		case let .dm(dm):
			return dm.createdAt
		}
	}

	@discardableResult public func send<T>(
		content: T, options: SendOptions? = nil, fallback _: String? = nil
	) async throws -> String {
		switch self {
		case let .group(group):
			return try await group.send(content: content, options: options)
		case let .dm(dm):
			return try await dm.send(content: content, options: options)
		}
	}

	@discardableResult public func send(
		encodedContent: EncodedContent
	) async throws -> String {
		switch self {
		case let .group(group):
			return try await group.send(
				encodedContent: encodedContent)
		case let .dm(dm):
			return try await dm.send(encodedContent: encodedContent)
		}
	}

	public func send(text: String, options: SendOptions? = nil) async throws
		-> String
	{
		switch self {
		case let .group(group):
			return try await group.send(content: text, options: options)
		case let .dm(dm):
			return try await dm.send(content: text, options: options)
		}
	}

	public var topic: String {
		switch self {
		case let .group(group):
			return group.topic
		case let .dm(dm):
			return dm.topic
		}
	}

	public func streamMessages(onClose: (() -> Void)? = nil) -> AsyncThrowingStream<DecodedMessage, Error> {
		switch self {
		case let .group(group):
			return group.streamMessages(onClose: onClose)
		case let .dm(dm):
			return dm.streamMessages(onClose: onClose)
		}
	}

	public func messages(
		limit: Int? = nil,
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all
	) async throws -> [DecodedMessage] {
		switch self {
		case let .group(group):
			return try await group.messages(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus
			)
		case let .dm(dm):
			return try await dm.messages(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus
			)
		}
	}

    // Returns null if conversation is not paused, otherwise the min version required to unpause this conversation
    public func pausedForVersion() async throws -> String? {
        switch self {
        case let .group(group):
            return try group.pausedForVersion()
        case let .dm(dm):
            return try dm.pausedForVersion()
        }
    }

	public var client: Client {
		switch self {
		case let .group(group):
			return group.client
		case let .dm(dm):
			return dm.client
		}
	}

	public func messagesWithReactions(
		limit: Int? = nil,
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all
	) async throws -> [DecodedMessage] {
		switch self {
		case let .group(group):
			return try await group.messagesWithReactions(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus
			)
		case let .dm(dm):
			return try await dm.messagesWithReactions(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus
			)
		}
	}

	public func messagesV2(
		limit: Int? = nil,
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all
	) async throws -> [DecodedMessageV2] {
		switch self {
		case let .group(group):
			return try await group.findMessagesV2(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus
			)
		case let .dm(dm):
			return try await dm.messagesV2(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus
			)
		}
	}

	public func getHmacKeys() throws -> Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse {
		switch self {
		case let .group(group):
			return try group.getHmacKeys()
		case let .dm(dm):
			return try dm.getHmacKeys()
		}
	}

    public func getPushTopics() async throws -> [String] {
        switch self {
        case let .group(group):
            return try group.getPushTopics()
        case let .dm(dm):
            return try await dm.getPushTopics()
        }
    }

	public func getDebugInformation() async throws -> ConversationDebugInfo  {
		switch self {
		case let .group(group):
			return try await group.getDebugInformation()
		case let .dm(dm):
			return try await dm.getDebugInformation()
		}
	}

	public func isActive() throws -> Bool {
		switch self {
		case let .group(group):
			return try group.isActive()
		case let .dm(dm):
			return try dm.isActive()
		}
	}
}
