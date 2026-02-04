import Foundation

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
				disappearingMessageSettings
			)
		case let .dm(dm):
			try await dm.updateDisappearingMessageSettings(
				disappearingMessageSettings
			)
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

	public func prepareMessage(
		encodedContent: EncodedContent,
		visibilityOptions: MessageVisibilityOptions? = nil,
		noSend: Bool = false
	) async throws
		-> String
	{
		switch self {
		case let .group(group):
			return try await group.prepareMessage(
				encodedContent: encodedContent,
				visibilityOptions: visibilityOptions,
				noSend: noSend
			)
		case let .dm(dm):
			return try await dm.prepareMessage(
				encodedContent: encodedContent,
				visibilityOptions: visibilityOptions,
				noSend: noSend
			)
		}
	}

	public func prepareMessage<T>(content: T, options: SendOptions? = nil, noSend: Bool = false)
		async throws -> String
	{
		switch self {
		case let .group(group):
			return try await group.prepareMessage(
				content: content, options: options, noSend: noSend
			)
		case let .dm(dm):
			return try await dm.prepareMessage(
				content: content, options: options, noSend: noSend
			)
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

	public func publishMessage(messageId: String) async throws {
		switch self {
		case let .group(group):
			return try await group.publishMessage(messageId: messageId)
		case let .dm(dm):
			return try await dm.publishMessage(messageId: messageId)
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

	public var createdAtNs: Int64 {
		switch self {
		case let .group(group):
			return group.createdAtNs
		case let .dm(dm):
			return dm.createdAtNs
		}
	}

	public var lastActivityAtNs: Int64 {
		switch self {
		case let .group(group):
			return group.lastActivityAtNs
		case let .dm(dm):
			return dm.lastActivityAtNs
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
		encodedContent: EncodedContent, visibilityOptions: MessageVisibilityOptions? = nil
	) async throws -> String {
		switch self {
		case let .group(group):
			return try await group.send(
				encodedContent: encodedContent, visibilityOptions: visibilityOptions
			)
		case let .dm(dm):
			return try await dm.send(
				encodedContent: encodedContent, visibilityOptions: visibilityOptions
			)
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

	public func streamMessages(onClose: (() -> Void)? = nil) -> AsyncThrowingStream<
		DecodedMessage, Error
	> {
		switch self {
		case let .group(group):
			return group.streamMessages(onClose: onClose)
		case let .dm(dm):
			return dm.streamMessages(onClose: onClose)
		}
	}

	/// Get the raw list of messages from a conversation.
	///
	/// This method returns all messages in chronological order without additional processing.
	/// Reactions, replies, and other associated metadata are returned as separate messages
	/// and are not linked to their parent messages.
	///
	/// For UI rendering, consider using ``enrichedMessages(limit:beforeNs:afterNs:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
	/// instead,
	/// which provides messages with enriched metadata automatically included.
	///
	/// - SeeAlso: ``enrichedMessages(limit:beforeNs:afterNs:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
	public func messages(
		limit: Int? = nil,
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all,
		excludeContentTypes: [StandardContentType]? = nil,
		excludeSenderInboxIds: [String]? = nil,
		sortBy: MessageSortBy? = nil,
		insertedAfterNs: Int64? = nil,
		insertedBeforeNs: Int64? = nil
	) async throws -> [DecodedMessage] {
		switch self {
		case let .group(group):
			return try await group.messages(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				sortBy: sortBy,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		case let .dm(dm):
			return try await dm.messages(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				sortBy: sortBy,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		}
	}

	/// Returns null if conversation is not paused, otherwise the min version required to unpause this conversation
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
		deliveryStatus: MessageDeliveryStatus = .all,
		excludeContentTypes: [StandardContentType]? = nil,
		excludeSenderInboxIds: [String]? = nil,
		sortBy: MessageSortBy? = nil,
		insertedAfterNs: Int64? = nil,
		insertedBeforeNs: Int64? = nil
	) async throws -> [DecodedMessage] {
		switch self {
		case let .group(group):
			return try await group.messagesWithReactions(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				sortBy: sortBy,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		case let .dm(dm):
			return try await dm.messagesWithReactions(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				sortBy: sortBy,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		}
	}

	/// Get messages with enriched metadata automatically included.
	///
	/// This method retrieves messages with reactions, replies, and other associated data
	/// "baked in" to each message, eliminating the need for separate queries to fetch
	/// this information.
	///
	/// **Recommended for UI rendering.** This method provides better performance and
	/// simpler code compared to ``messages(limit:beforeNs:afterNs:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
	/// when displaying conversations.
	///
	/// When handling content types, use the generic `content<T>()` method with the
	/// appropriate type for reactions and replies.
	///
	/// - Returns: Array of `DecodedMessageV2` with enriched metadata.
	/// - SeeAlso: ``messages(limit:beforeNs:afterNs:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
	public func enrichedMessages(
		limit: Int? = nil,
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all,
		excludeContentTypes: [StandardContentType]? = nil,
		excludeSenderInboxIds: [String]? = nil,
		sortBy: MessageSortBy? = nil,
		insertedAfterNs: Int64? = nil,
		insertedBeforeNs: Int64? = nil
	) async throws -> [DecodedMessageV2] {
		switch self {
		case let .group(group):
			return try await group.enrichedMessages(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				sortBy: sortBy,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		case let .dm(dm):
			return try await dm.enrichedMessages(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				sortBy: sortBy,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		}
	}

	public func countMessages(
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		deliveryStatus: MessageDeliveryStatus = .all,
		excludeContentTypes: [StandardContentType]? = nil,
		excludeSenderInboxIds: [String]? = nil,
		insertedAfterNs: Int64? = nil,
		insertedBeforeNs: Int64? = nil
	) throws -> Int64 {
		switch self {
		case let .group(group):
			return try group.countMessages(
				beforeNs: beforeNs, afterNs: afterNs, deliveryStatus: deliveryStatus,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		case let .dm(dm):
			return try dm.countMessages(
				beforeNs: beforeNs, afterNs: afterNs, deliveryStatus: deliveryStatus,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
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

	public func getDebugInformation() async throws -> ConversationDebugInfo {
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

	/// Returns a dictionary where the keys are inbox IDs and the values
	/// are the timestamp in nanoseconds of their last read receipt
	public func getLastReadTimes() throws -> [String: Int64] {
		switch self {
		case let .group(group):
			return try group.getLastReadTimes()
		case let .dm(dm):
			return try dm.getLastReadTimes()
		}
	}

	/// Delete a message by its ID.
	/// - Parameter messageId: The hex-encoded message ID to delete.
	/// - Returns: The hex-encoded ID of the deletion message.
	/// - Throws: An error if the deletion fails (e.g., unauthorized deletion).
	public func deleteMessage(messageId: String) async throws -> String {
		switch self {
		case let .group(group):
			return try await group.deleteMessage(messageId: messageId)
		case let .dm(dm):
			return try await dm.deleteMessage(messageId: messageId)
		}
	}
}
