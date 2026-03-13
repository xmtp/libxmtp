import Foundation

/// A polymorphic wrapper over ``Group`` and ``Dm`` that provides a unified API surface
/// for working with conversations regardless of their underlying type.
///
/// Use `Conversation` when you want to handle groups and direct messages interchangeably.
/// Each method delegates to the corresponding method on the underlying ``Group`` or ``Dm``.
public enum Conversation: Identifiable, Equatable, Hashable {
	/// A group conversation with multiple participants.
	case group(Group)
	/// A direct message conversation between two participants.
	case dm(Dm)

	public static func == (lhs: Conversation, rhs: Conversation) -> Bool {
		lhs.topic == rhs.topic
	}

	public func hash(into hasher: inout Hasher) {
		hasher.combine(topic)
	}

	/// The type of conversation.
	public enum XMTPConversationType {
		/// A group conversation with multiple participants.
		case group
		/// A direct message conversation between two participants.
		case dm
	}

	/// The unique identifier for this conversation.
	public var id: String {
		switch self {
		case let .group(group):
			group.id
		case let .dm(dm):
			dm.id
		}
	}

	/// The current disappearing message settings for this conversation, or `nil` if not configured.
	public var disappearingMessageSettings: DisappearingMessageSettings? {
		switch self {
		case let .group(group):
			group.disappearingMessageSettings
		case let .dm(dm):
			dm.disappearingMessageSettings
		}
	}

	/// Returns whether disappearing messages are enabled for this conversation.
	public func isDisappearingMessagesEnabled() throws -> Bool {
		switch self {
		case let .group(group):
			try group.isDisappearingMessagesEnabled()
		case let .dm(dm):
			try dm.isDisappearingMessagesEnabled()
		}
	}

	/// Returns the most recent message in this conversation, or `nil` if the conversation is empty.
	public func lastMessage() async throws -> DecodedMessage? {
		switch self {
		case let .group(group):
			try await group.lastMessage()
		case let .dm(dm):
			try await dm.lastMessage()
		}
	}

	/// Returns the fork status of the conversation's commit log, indicating whether the MLS group state has diverged.
	public func commitLogForkStatus() -> CommitLogForkStatus {
		switch self {
		case let .group(group):
			group.commitLogForkStatus()
		case let .dm(dm):
			dm.commitLogForkStatus()
		}
	}

	/// Returns whether the current client created this conversation.
	public func isCreator() async throws -> Bool {
		switch self {
		case let .group(group):
			try await group.isCreator()
		case let .dm(dm):
			try await dm.isCreator()
		}
	}

	/// Returns the list of members in this conversation.
	public func members() async throws -> [Member] {
		switch self {
		case let .group(group):
			try await group.members
		case let .dm(dm):
			try await dm.members
		}
	}

	/// Returns the current consent state (allowed, denied, or unknown) for this conversation.
	public func consentState() throws -> ConsentState {
		switch self {
		case let .group(group):
			try group.consentState()
		case let .dm(dm):
			try dm.consentState()
		}
	}

	/// Updates the consent state for this conversation.
	///
	/// - Parameter state: The new consent state to apply.
	public func updateConsentState(state: ConsentState) async throws {
		switch self {
		case let .group(group):
			try await group.updateConsentState(state: state)
		case let .dm(dm):
			try await dm.updateConsentState(state: state)
		}
	}

	/// Updates the disappearing message settings for this conversation.
	///
	/// - Parameter disappearingMessageSettings: The new settings to apply, or `nil` to disable.
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

	/// Clears the disappearing message settings, disabling disappearing messages for this conversation.
	public func clearDisappearingMessageSettings() async throws {
		switch self {
		case let .group(group):
			try await group.clearDisappearingMessageSettings()
		case let .dm(dm):
			try await dm.clearDisappearingMessageSettings()
		}
	}

	/// Syncs this conversation with the network, fetching new messages and state updates.
	public func sync() async throws {
		switch self {
		case let .group(group):
			try await group.sync()
		case let .dm(dm):
			try await dm.sync()
		}
	}

	/// Decrypts and processes a raw message received via push notification or other out-of-band delivery.
	///
	/// - Parameter messageBytes: The raw encrypted message bytes.
	/// - Returns: The decoded message, or `nil` if the message could not be processed.
	public func processMessage(messageBytes: Data) async throws -> DecodedMessage? {
		switch self {
		case let .group(group):
			try await group.processMessage(messageBytes: messageBytes)
		case let .dm(dm):
			try await dm.processMessage(messageBytes: messageBytes)
		}
	}

	/// Prepares a message from pre-encoded content for later publishing.
	///
	/// - Parameters:
	///   - encodedContent: The pre-encoded content to send.
	///   - visibilityOptions: Optional visibility settings for the message.
	///   - noSend: If `true`, the message is prepared but not queued for sending.
	/// - Returns: The hex-encoded message ID of the prepared message.
	public func prepareMessage(
		encodedContent: EncodedContent,
		visibilityOptions: MessageVisibilityOptions? = nil,
		noSend: Bool = false
	) async throws
		-> String
	{
		switch self {
		case let .group(group):
			try await group.prepareMessage(
				encodedContent: encodedContent,
				visibilityOptions: visibilityOptions,
				noSend: noSend
			)
		case let .dm(dm):
			try await dm.prepareMessage(
				encodedContent: encodedContent,
				visibilityOptions: visibilityOptions,
				noSend: noSend
			)
		}
	}

	/// Prepares a message from any content type for later publishing.
	///
	/// - Parameters:
	///   - content: The content to send, which will be encoded using the appropriate content codec.
	///   - options: Optional send options controlling encoding behavior.
	///   - noSend: If `true`, the message is prepared but not queued for sending.
	/// - Returns: The hex-encoded message ID of the prepared message.
	public func prepareMessage(content: some Any, options: SendOptions? = nil, noSend: Bool = false)
		async throws -> String
	{
		switch self {
		case let .group(group):
			try await group.prepareMessage(
				content: content, options: options, noSend: noSend
			)
		case let .dm(dm):
			try await dm.prepareMessage(
				content: content, options: options, noSend: noSend
			)
		}
	}

	/// Publishes all pending prepared messages to the network.
	public func publishMessages() async throws {
		switch self {
		case let .group(group):
			try await group.publishMessages()
		case let .dm(dm):
			try await dm.publishMessages()
		}
	}

	/// Publishes a single prepared message to the network.
	///
	/// - Parameter messageId: The hex-encoded message ID of the prepared message to publish.
	public func publishMessage(messageId: String) async throws {
		switch self {
		case let .group(group):
			try await group.publishMessage(messageId: messageId)
		case let .dm(dm):
			try await dm.publishMessage(messageId: messageId)
		}
	}

	/// The type of this conversation (group or dm).
	public var type: XMTPConversationType {
		switch self {
		case .group:
			.group
		case .dm:
			.dm
		}
	}

	/// The date when this conversation was created.
	public var createdAt: Date {
		switch self {
		case let .group(group):
			group.createdAt
		case let .dm(dm):
			dm.createdAt
		}
	}

	/// The creation timestamp of this conversation in nanoseconds since the Unix epoch.
	public var createdAtNs: Int64 {
		switch self {
		case let .group(group):
			group.createdAtNs
		case let .dm(dm):
			dm.createdAtNs
		}
	}

	/// The timestamp of the last activity in this conversation in nanoseconds since the Unix epoch.
	public var lastActivityAtNs: Int64 {
		switch self {
		case let .group(group):
			group.lastActivityAtNs
		case let .dm(dm):
			dm.lastActivityAtNs
		}
	}

	/// Sends a message with any content type to this conversation.
	///
	/// - Parameters:
	///   - content: The content to send, which will be encoded using the appropriate content codec.
	///   - options: Optional send options controlling encoding behavior.
	/// - Returns: The hex-encoded message ID of the sent message.
	@discardableResult public func send(
		content: some Any, options: SendOptions? = nil, fallback _: String? = nil
	) async throws -> String {
		switch self {
		case let .group(group):
			try await group.send(content: content, options: options)
		case let .dm(dm):
			try await dm.send(content: content, options: options)
		}
	}

	/// Sends a pre-encoded message to this conversation.
	///
	/// - Parameters:
	///   - encodedContent: The pre-encoded content to send.
	///   - visibilityOptions: Optional visibility settings for the message.
	/// - Returns: The hex-encoded message ID of the sent message.
	@discardableResult public func send(
		encodedContent: EncodedContent, visibilityOptions: MessageVisibilityOptions? = nil
	) async throws -> String {
		switch self {
		case let .group(group):
			try await group.send(
				encodedContent: encodedContent, visibilityOptions: visibilityOptions
			)
		case let .dm(dm):
			try await dm.send(
				encodedContent: encodedContent, visibilityOptions: visibilityOptions
			)
		}
	}

	/// Sends a plain text message to this conversation.
	///
	/// - Parameters:
	///   - text: The text string to send.
	///   - options: Optional send options controlling encoding behavior.
	/// - Returns: The hex-encoded message ID of the sent message.
	public func send(text: String, options: SendOptions? = nil) async throws
		-> String
	{
		switch self {
		case let .group(group):
			try await group.send(content: text, options: options)
		case let .dm(dm):
			try await dm.send(content: text, options: options)
		}
	}

	/// The MLS group topic identifier for this conversation, used for network-level message routing.
	public var topic: String {
		switch self {
		case let .group(group):
			group.topic
		case let .dm(dm):
			dm.topic
		}
	}

	/// Returns an asynchronous stream of new messages arriving in this conversation.
	///
	/// - Parameter onClose: An optional closure called when the stream closes.
	/// - Returns: An `AsyncThrowingStream` that yields each new ``DecodedMessage`` as it arrives.
	public func streamMessages(onClose: (() -> Void)? = nil) -> AsyncThrowingStream<
		DecodedMessage, Error
	> {
		switch self {
		case let .group(group):
			group.streamMessages(onClose: onClose)
		case let .dm(dm):
			dm.streamMessages(onClose: onClose)
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
			try await group.messages(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				sortBy: sortBy,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		case let .dm(dm):
			try await dm.messages(
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

	/// Returns `nil` if this conversation is not paused, otherwise the minimum version required to unpause it.
	public func pausedForVersion() async throws -> String? {
		switch self {
		case let .group(group):
			try group.pausedForVersion()
		case let .dm(dm):
			try dm.pausedForVersion()
		}
	}

	/// The ``Client`` instance that owns this conversation.
	public var client: Client {
		switch self {
		case let .group(group):
			group.client
		case let .dm(dm):
			dm.client
		}
	}

	/// Returns messages with reactions attached as child messages.
	///
	/// This is a legacy method; prefer ``enrichedMessages(limit:beforeNs:afterNs:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
	/// for new code.
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
			try await group.messagesWithReactions(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				sortBy: sortBy,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		case let .dm(dm):
			try await dm.messagesWithReactions(
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
			try await group.enrichedMessages(
				beforeNs: beforeNs, afterNs: afterNs, limit: limit,
				direction: direction, deliveryStatus: deliveryStatus,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				sortBy: sortBy,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		case let .dm(dm):
			try await dm.enrichedMessages(
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

	/// Returns the number of messages in this conversation matching the given filters.
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
			try group.countMessages(
				beforeNs: beforeNs, afterNs: afterNs, deliveryStatus: deliveryStatus,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		case let .dm(dm):
			try dm.countMessages(
				beforeNs: beforeNs, afterNs: afterNs, deliveryStatus: deliveryStatus,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		}
	}

	/// Returns the HMAC keys for this conversation, used for push notification decryption.
	public func getHmacKeys() throws -> Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse {
		switch self {
		case let .group(group):
			try group.getHmacKeys()
		case let .dm(dm):
			try dm.getHmacKeys()
		}
	}

	/// Returns the topic strings to subscribe to for push notifications on this conversation.
	public func getPushTopics() async throws -> [String] {
		switch self {
		case let .group(group):
			try group.getPushTopics()
		case let .dm(dm):
			try await dm.getPushTopics()
		}
	}

	/// Returns debug information about this conversation's internal state.
	public func getDebugInformation() async throws -> ConversationDebugInfo {
		switch self {
		case let .group(group):
			try await group.getDebugInformation()
		case let .dm(dm):
			try await dm.getDebugInformation()
		}
	}

	/// Returns whether this conversation is active (not removed or inactive in the MLS group).
	public func isActive() throws -> Bool {
		switch self {
		case let .group(group):
			try group.isActive()
		case let .dm(dm):
			try dm.isActive()
		}
	}

	/// Returns a dictionary where the keys are inbox IDs and the values
	/// are the timestamp in nanoseconds of their last read receipt
	public func getLastReadTimes() throws -> [String: Int64] {
		switch self {
		case let .group(group):
			try group.getLastReadTimes()
		case let .dm(dm):
			try dm.getLastReadTimes()
		}
	}

	/// Delete a message by its ID.
	/// - Parameter messageId: The hex-encoded message ID to delete.
	/// - Returns: The hex-encoded ID of the deletion message.
	/// - Throws: An error if the deletion fails (e.g., unauthorized deletion).
	public func deleteMessage(messageId: String) async throws -> String {
		switch self {
		case let .group(group):
			try await group.deleteMessage(messageId: messageId)
		case let .dm(dm):
			try await dm.deleteMessage(messageId: messageId)
		}
	}
}
