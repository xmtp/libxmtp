import Foundation

/// A 1:1 direct message conversation between two participants.
///
/// `Dm` represents a private conversation between the current client and exactly one
/// other user, identified by their inbox ID. Unlike ``Group``, a DM has no admin
/// operations, member management, or metadata like name or image.
///
/// Use ``Conversations/newDm(with:)`` to create a new DM, or retrieve existing ones
/// through ``Conversations/listDms()``.
public struct Dm: Identifiable, Equatable, Hashable {
	var ffiConversation: FfiConversation
	var ffiLastMessage: FfiMessage?
	var ffiCommitLogForkStatus: Bool?
	var client: Client
	let streamHolder = StreamHolder()

	/// Errors specific to direct message conversations.
	public enum ConversationError: Error, CustomStringConvertible, LocalizedError {
		/// The DM does not have a peer inbox ID, indicating a malformed conversation.
		case missingPeerInboxId

		public var description: String {
			switch self {
			case .missingPeerInboxId:
				"ConversationError.missingPeerInboxId: The direct message is missing a peer inbox ID"
			}
		}

		public var errorDescription: String? {
			description
		}
	}

	/// The unique hex-encoded identifier for this DM conversation.
	public var id: String {
		ffiConversation.id().toHex
	}

	/// The topic string used for subscribing to messages in this DM.
	public var topic: String {
		Topic.groupMessage(id).description
	}

	/// The current disappearing message settings for this DM, or `nil` if disappearing messages are disabled.
	public var disappearingMessageSettings: DisappearingMessageSettings? {
		try? {
			guard try isDisappearingMessagesEnabled() else { return nil }
			return try ffiConversation.conversationMessageDisappearingSettings()
				.map { DisappearingMessageSettings.createFromFfi($0) }
		}()
	}

	/// Returns whether disappearing messages are enabled for this DM.
	///
	/// - Returns: `true` if messages in this DM are configured to disappear after a set duration.
	/// - Throws: If the conversation metadata cannot be read.
	public func isDisappearingMessagesEnabled() throws -> Bool {
		try ffiConversation.isConversationMessageDisappearingEnabled()
	}

	func metadata() async throws -> FfiConversationMetadata {
		try await ffiConversation.groupMetadata()
	}

	/// Synchronizes the DM state with the network.
	///
	/// Call this to pull the latest messages and metadata from the XMTP network.
	/// This is required after being offline or to ensure the local state is up to date.
	///
	/// - Throws: If the network request fails or the conversation state cannot be updated.
	public func sync() async throws {
		try await ffiConversation.sync()
	}

	public static func == (lhs: Dm, rhs: Dm) -> Bool {
		lhs.id == rhs.id
	}

	public func hash(into hasher: inout Hasher) {
		id.hash(into: &hasher)
	}

	/// Returns whether the current client initiated this DM.
	///
	/// - Returns: `true` if the current client's inbox ID matches the DM creator.
	/// - Throws: If the conversation metadata cannot be retrieved.
	public func isCreator() async throws -> Bool {
		try await metadata().creatorInboxId() == client.inboxID
	}

	/// Returns whether this DM is currently active.
	///
	/// - Returns: `true` if the DM is active and can send or receive messages.
	/// - Throws: If the conversation state cannot be read.
	public func isActive() throws -> Bool {
		try ffiConversation.isActive()
	}

	/// Returns the inbox ID of the participant who created this DM.
	///
	/// - Returns: The ``InboxId`` of the DM creator.
	/// - Throws: If the conversation metadata cannot be retrieved.
	public func creatorInboxId() async throws -> InboxId {
		try await metadata().creatorInboxId()
	}

	/// Returns the inbox ID of the participant who added this client to the DM.
	///
	/// - Returns: The ``InboxId`` of the member who initiated the DM with this client.
	/// - Throws: If the conversation metadata cannot be read.
	public func addedByInboxId() throws -> InboxId {
		try ffiConversation.addedByInboxId()
	}

	/// The two members of this DM: the current client and the peer.
	public var members: [Member] {
		get async throws {
			try await ffiConversation.listMembers().map {
				ffiGroupMember in
				Member(ffiGroupMember: ffiGroupMember)
			}
		}
	}

	/// The inbox ID of the other participant in this DM.
	///
	/// - Throws: ``ConversationError/missingPeerInboxId`` if the peer cannot be determined.
	public var peerInboxId: InboxId {
		get throws {
			guard let inboxId = ffiConversation.dmPeerInboxId() else {
				throw ConversationError.missingPeerInboxId
			}
			return inboxId
		}
	}

	/// The date when this DM was created.
	public var createdAt: Date {
		Date(millisecondsSinceEpoch: ffiConversation.createdAtNs())
	}

	/// The creation timestamp of this DM in nanoseconds since the Unix epoch.
	public var createdAtNs: Int64 {
		ffiConversation.createdAtNs()
	}

	/// The timestamp of the last activity in this DM in nanoseconds since the Unix epoch.
	///
	/// Returns the sent time of the most recent message, or the creation timestamp if no messages exist.
	public var lastActivityAtNs: Int64 {
		ffiLastMessage?.sentAtNs ?? createdAtNs
	}

	/// Updates the consent state for this DM.
	///
	/// Use this to allow, deny, or reset the consent state for messages from the peer.
	///
	/// - Parameter state: The new consent state to apply.
	/// - Throws: If the consent state cannot be updated.
	public func updateConsentState(state: ConsentState) async throws {
		try ffiConversation.updateConsentState(state: state.toFFI)
	}

	/// Returns the current consent state for this DM.
	///
	/// - Returns: The current ``ConsentState`` (e.g., `.allowed`, `.denied`, `.unknown`).
	/// - Throws: If the consent state cannot be read.
	public func consentState() throws -> ConsentState {
		try ffiConversation.consentState().fromFFI
	}

	/// Updates the disappearing message settings for this DM.
	///
	/// Pass `nil` to clear the settings and disable disappearing messages.
	///
	/// - Parameter disappearingMessageSettings: The new settings, or `nil` to disable.
	/// - Throws: If the settings cannot be updated.
	public func updateDisappearingMessageSettings(
		_ disappearingMessageSettings: DisappearingMessageSettings?
	) async throws {
		if let settings = disappearingMessageSettings {
			let ffiSettings = FfiMessageDisappearingSettings(
				fromNs: settings.disappearStartingAtNs,
				inNs: settings.retentionDurationInNs
			)
			try await ffiConversation
				.updateConversationMessageDisappearingSettings(
					settings: ffiSettings
				)
		} else {
			try await clearDisappearingMessageSettings()
		}
	}

	/// Removes disappearing message settings, disabling the feature for this DM.
	///
	/// - Throws: If the settings cannot be cleared.
	public func clearDisappearingMessageSettings() async throws {
		try await ffiConversation.removeConversationMessageDisappearingSettings()
	}

	/// Returns null if dm is not paused, otherwise the min version required to unpause this dm
	public func pausedForVersion() throws -> String? {
		try ffiConversation.pausedForVersion()
	}

	/// Processes an incoming message from a push notification payload.
	///
	/// Decrypts and decodes a raw message envelope received via push notifications
	/// and returns the decoded message if valid.
	///
	/// - Parameter messageBytes: The raw message bytes from the push notification.
	/// - Returns: The decoded message, or `nil` if the payload could not be decoded.
	/// - Throws: If decryption or processing fails.
	public func processMessage(messageBytes: Data) async throws -> DecodedMessage? {
		let messages =
			try await ffiConversation.processStreamedConversationMessage(
				envelopeBytes: messageBytes
			)
		guard let firstMessage = messages.first else {
			return nil
		}
		return DecodedMessage.create(ffiMessage: firstMessage)
	}

	/// Sends a message to the peer in this DM.
	///
	/// Encodes the content using the appropriate codec and sends it. The codec is
	/// selected based on the content type specified in `options`, or the default
	/// codec if none is specified.
	///
	/// - Parameters:
	///   - content: The message content to send (e.g., a `String` for text messages).
	///   - options: Optional send options including content type and compression.
	/// - Returns: The hex-encoded message ID of the sent message.
	/// - Throws: ``CodecError/invalidContent`` if the content does not match the codec type.
	public func send(content: some Any, options: SendOptions? = nil) async throws
		-> String
	{
		let (encodeContent, visibilityOptions) = try await encodeContent(
			content: content, options: options
		)
		return try await send(encodedContent: encodeContent, visibilityOptions: visibilityOptions)
	}

	/// Sends pre-encoded content to the peer in this DM.
	///
	/// Use this overload when you have already encoded the content yourself,
	/// for example when re-sending a previously encoded message.
	///
	/// - Parameters:
	///   - encodedContent: The pre-encoded message content.
	///   - visibilityOptions: Optional visibility options such as whether to trigger a push notification.
	/// - Returns: The hex-encoded message ID of the sent message.
	/// - Throws: If serialization or sending fails.
	public func send(
		encodedContent: EncodedContent, visibilityOptions: MessageVisibilityOptions? = nil
	) async throws -> String {
		let opts = visibilityOptions?.toFfi() ?? FfiSendMessageOpts(shouldPush: true)
		let messageId = try await ffiConversation.send(
			contentBytes: encodedContent.serializedData(),
			opts: opts
		)
		return messageId.toHex
	}

	/// Encodes content without sending it.
	///
	/// Useful for preparing a message payload in advance, for example to display
	/// an optimistic UI before calling ``send(encodedContent:visibilityOptions:)``.
	///
	/// - Parameters:
	///   - content: The message content to encode.
	///   - options: Optional send options including content type and compression.
	/// - Returns: A tuple of the encoded content and the resolved visibility options.
	/// - Throws: ``CodecError/invalidContent`` if the content does not match the codec type.
	public func encodeContent<T>(content: T, options: SendOptions?) async throws
		-> (EncodedContent, MessageVisibilityOptions)
	{
		let codec = Client.codecRegistry.find(for: options?.contentType)

		func encode<Codec: ContentCodec>(codec: Codec, content: Any) throws
			-> EncodedContent
		{
			if let content = content as? Codec.T {
				return try codec.encode(content: content)
			} else {
				throw CodecError.invalidContent
			}
		}

		var encoded = try encode(codec: codec, content: content)

		func fallback<Codec: ContentCodec>(codec: Codec, content: Any) throws
			-> String?
		{
			if let content = content as? Codec.T {
				return try codec.fallback(content: content)
			} else {
				throw CodecError.invalidContent
			}
		}

		if let fallback = try fallback(codec: codec, content: content) {
			encoded.fallback = fallback
		}

		if let compression = options?.compression {
			encoded = try encoded.compress(compression)
		}

		func shouldPush<Codec: ContentCodec>(codec: Codec, content: Any) throws
			-> Bool
		{
			if let content = content as? Codec.T {
				return try codec.shouldPush(content: content)
			} else {
				throw CodecError.invalidContent
			}
		}

		let visibilityOptions = try MessageVisibilityOptions(
			shouldPush: shouldPush(codec: codec, content: content)
		)

		return (encoded, visibilityOptions)
	}

	/// Prepares a pre-encoded message for sending, optionally deferring publication.
	///
	/// When `noSend` is `false` (the default), the message is stored locally and
	/// sent optimistically. When `true`, the message is stored locally but not
	/// published to the network until ``publishMessages()`` or
	/// ``publishMessage(messageId:)`` is called.
	///
	/// - Parameters:
	///   - encodedContent: The pre-encoded message content.
	///   - visibilityOptions: Optional visibility options such as whether to trigger a push notification.
	///   - noSend: If `true`, stores the message locally without publishing to the network.
	/// - Returns: The hex-encoded message ID.
	/// - Throws: If message preparation or local storage fails.
	public func prepareMessage(
		encodedContent: EncodedContent,
		visibilityOptions: MessageVisibilityOptions? = nil,
		noSend: Bool = false
	) async throws
		-> String
	{
		let shouldPush = visibilityOptions?.shouldPush ?? true
		let messageId: Data
		if noSend {
			messageId = try ffiConversation.prepareMessage(
				contentBytes: encodedContent.serializedData(),
				shouldPush: shouldPush
			)
		} else {
			let opts = visibilityOptions?.toFfi() ?? FfiSendMessageOpts(shouldPush: true)
			messageId = try ffiConversation.sendOptimistic(
				contentBytes: encodedContent.serializedData(),
				opts: opts
			)
		}
		return messageId.toHex
	}

	/// Prepares a message for sending by encoding the content, optionally deferring publication.
	///
	/// This is a convenience overload that encodes the content before preparing.
	/// See ``prepareMessage(encodedContent:visibilityOptions:noSend:)`` for details
	/// on the `noSend` behavior.
	///
	/// - Parameters:
	///   - content: The message content to encode and prepare.
	///   - options: Optional send options including content type and compression.
	///   - noSend: If `true`, stores the message locally without publishing to the network.
	/// - Returns: The hex-encoded message ID.
	/// - Throws: ``CodecError/invalidContent`` if the content does not match the codec type.
	public func prepareMessage(content: some Any, options: SendOptions? = nil, noSend: Bool = false)
		async throws -> String
	{
		let (encodeContent, visibilityOptions) = try await encodeContent(
			content: content, options: options
		)
		return try await prepareMessage(
			encodedContent: encodeContent,
			visibilityOptions: visibilityOptions,
			noSend: noSend
		)
	}

	/// Publishes all locally stored unpublished messages to the network.
	///
	/// Call this after using ``prepareMessage(encodedContent:visibilityOptions:noSend:)``
	/// with `noSend: true` to send all pending messages at once.
	///
	/// - Throws: If publishing fails.
	public func publishMessages() async throws {
		try await ffiConversation.publishMessages()
	}

	/// Publishes a single locally stored message to the network by its ID.
	///
	/// - Parameter messageId: The hex-encoded ID of the message to publish.
	/// - Throws: If the message cannot be found or publishing fails.
	public func publishMessage(messageId: String) async throws {
		try await ffiConversation.publishStoredMessage(messageId: messageId.hexToData)
	}

	/// Ends the active message stream for this DM.
	///
	/// After calling this, the `AsyncThrowingStream` returned by ``streamMessages(onClose:)``
	/// will finish.
	public func endStream() {
		streamHolder.stream?.end()
	}

	/// Returns a stream of new messages arriving in this DM in real time.
	///
	/// The stream stays open until ``endStream()`` is called, the task is cancelled,
	/// or the connection is closed by the server.
	///
	/// - Parameter onClose: An optional closure invoked when the stream terminates.
	/// - Returns: An `AsyncThrowingStream` that yields each new ``DecodedMessage`` as it arrives.
	public func streamMessages(onClose: (() -> Void)? = nil) -> AsyncThrowingStream<
		DecodedMessage, Error
	> {
		AsyncThrowingStream { continuation in
			let task = Task.detached {
				streamHolder.stream = await ffiConversation.stream(
					messageCallback: MessageCallback {
						message in
						guard !Task.isCancelled else {
							continuation.finish()
							return
						}
						if let message = DecodedMessage.create(ffiMessage: message) {
							continuation.yield(message)
						}
					} onClose: {
						onClose?()
						continuation.finish()
					}
				)

				continuation.onTermination = { @Sendable _ in
					streamHolder.stream?.end()
				}
			}

			continuation.onTermination = { @Sendable _ in
				task.cancel()
				streamHolder.stream?.end()
			}
		}
	}

	/// Returns the most recent message in this DM, or `nil` if the conversation is empty.
	///
	/// Uses a cached value when available, otherwise queries the local store.
	///
	/// - Returns: The most recent ``DecodedMessage``, or `nil`.
	/// - Throws: If reading from the local store fails.
	public func lastMessage() async throws -> DecodedMessage? {
		if let ffiMessage = ffiLastMessage {
			DecodedMessage.create(ffiMessage: ffiMessage)
		} else {
			try await messages(limit: 1).first
		}
	}

	/// Returns the fork status of this DM's commit log.
	///
	/// A forked commit log indicates a divergence in the conversation's MLS state.
	///
	/// - Returns: `.forked`, `.notForked`, or `.unknown` if the status has not been determined.
	public func commitLogForkStatus() -> CommitLogForkStatus {
		switch ffiCommitLogForkStatus {
		case true: .forked
		case false: .notForked
		default: .unknown
		}
	}

	/// Get the raw list of messages from a conversation.
	///
	/// This method returns all messages in chronological order without additional processing.
	/// Reactions, replies, and other associated metadata are returned as separate messages
	/// and are not linked to their parent messages.
	///
	/// For UI rendering, consider using ``enrichedMessages(beforeNs:afterNs:limit:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
	/// instead,
	/// which provides messages with enriched metadata automatically included.
	///
	/// - SeeAlso: ``enrichedMessages(beforeNs:afterNs:limit:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
	public func messages(
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		limit: Int? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all,
		excludeContentTypes: [StandardContentType]? = nil,
		excludeSenderInboxIds: [String]? = nil,
		sortBy: MessageSortBy? = nil,
		insertedAfterNs: Int64? = nil,
		insertedBeforeNs: Int64? = nil
	) async throws -> [DecodedMessage] {
		var options = FfiListMessagesOptions(
			sentBeforeNs: nil,
			sentAfterNs: nil,
			limit: nil,
			deliveryStatus: nil,
			direction: nil,
			contentTypes: nil,
			excludeContentTypes: nil,
			excludeSenderInboxIds: nil,
			sortBy: nil,
			insertedAfterNs: nil,
			insertedBeforeNs: nil
		)

		if let beforeNs {
			options.sentBeforeNs = beforeNs
		}

		if let afterNs {
			options.sentAfterNs = afterNs
		}

		if let limit {
			options.limit = Int64(limit)
		}

		let status: FfiDeliveryStatus? = switch deliveryStatus {
		case .published:
			FfiDeliveryStatus.published
		case .unpublished:
			FfiDeliveryStatus.unpublished
		case .failed:
			FfiDeliveryStatus.failed
		default:
			nil
		}

		options.deliveryStatus = status

		let direction: FfiDirection? = switch direction {
		case .ascending:
			FfiDirection.ascending
		default:
			FfiDirection.descending
		}

		options.direction = direction
		options.excludeContentTypes = excludeContentTypes
		options.excludeSenderInboxIds = excludeSenderInboxIds
		options.sortBy = sortBy?.toFfi()
		options.insertedAfterNs = insertedAfterNs
		options.insertedBeforeNs = insertedBeforeNs

		return try await ffiConversation.findMessages(opts: options).compactMap {
			ffiMessage in
			DecodedMessage.create(ffiMessage: ffiMessage)
		}
	}

	/// Returns messages with their associated reactions included inline.
	///
	/// Unlike ``messages(beforeNs:afterNs:limit:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``,
	/// this method attaches reaction data directly to each message. For a richer
	/// result that also includes replies and other metadata, prefer
	/// ``enrichedMessages(beforeNs:afterNs:limit:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``.
	public func messagesWithReactions(
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		limit: Int? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all,
		excludeContentTypes: [StandardContentType]? = nil,
		excludeSenderInboxIds: [String]? = nil,
		sortBy: MessageSortBy? = nil,
		insertedAfterNs: Int64? = nil,
		insertedBeforeNs: Int64? = nil
	) async throws -> [DecodedMessage] {
		var options = FfiListMessagesOptions(
			sentBeforeNs: nil,
			sentAfterNs: nil,
			limit: nil,
			deliveryStatus: nil,
			direction: nil,
			contentTypes: nil,
			excludeContentTypes: nil,
			excludeSenderInboxIds: nil,
			sortBy: nil,
			insertedAfterNs: nil,
			insertedBeforeNs: nil
		)

		if let beforeNs {
			options.sentBeforeNs = beforeNs
		}

		if let afterNs {
			options.sentAfterNs = afterNs
		}

		if let limit {
			options.limit = Int64(limit)
		}

		options.deliveryStatus = deliveryStatus.toFfi()

		let direction: FfiDirection? = switch direction {
		case .ascending:
			FfiDirection.ascending
		default:
			FfiDirection.descending
		}

		options.direction = direction
		options.excludeContentTypes = excludeContentTypes
		options.excludeSenderInboxIds = excludeSenderInboxIds
		options.sortBy = sortBy?.toFfi()
		options.insertedAfterNs = insertedAfterNs
		options.insertedBeforeNs = insertedBeforeNs

		return try ffiConversation.findMessagesWithReactions(
			opts: options
		).compactMap {
			ffiMessageWithReactions in
			DecodedMessage.create(ffiMessage: ffiMessageWithReactions)
		}
	}

	/// Count the number of messages in the conversation according to the provided filters
	public func countMessages(
		beforeNs: Int64? = nil, afterNs: Int64? = nil, deliveryStatus: MessageDeliveryStatus = .all,
		excludeContentTypes: [StandardContentType]? = nil,
		excludeSenderInboxIds: [String]? = nil,
		insertedAfterNs: Int64? = nil,
		insertedBeforeNs: Int64? = nil
	) throws -> Int64 {
		try ffiConversation.countMessages(
			opts: FfiListMessagesOptions(
				sentBeforeNs: beforeNs,
				sentAfterNs: afterNs,
				limit: nil,
				deliveryStatus: deliveryStatus.toFfi(),
				direction: .descending,
				contentTypes: nil,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				sortBy: nil,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		)
	}

	/// Get messages with enriched metadata automatically included.
	///
	/// This method retrieves messages with reactions, replies, and other associated data
	/// "baked in" to each message, eliminating the need for separate queries to fetch
	/// this information.
	///
	/// **Recommended for UI rendering.** This method provides better performance and
	/// simpler code compared to ``messages(beforeNs:afterNs:limit:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
	/// when displaying conversations.
	///
	/// When handling content types, use the generic `content<T>()` method with the
	/// appropriate type for reactions and replies.
	///
	/// - Returns: Array of `DecodedMessageV2` with enriched metadata.
	/// - SeeAlso: ``messages(beforeNs:afterNs:limit:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
	public func enrichedMessages(
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		limit: Int? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all,
		excludeContentTypes: [StandardContentType]? = nil,
		excludeSenderInboxIds: [String]? = nil,
		sortBy: MessageSortBy? = nil,
		insertedAfterNs: Int64? = nil,
		insertedBeforeNs: Int64? = nil
	) async throws -> [DecodedMessageV2] {
		var options = FfiListMessagesOptions(
			sentBeforeNs: nil,
			sentAfterNs: nil,
			limit: nil,
			deliveryStatus: nil,
			direction: nil,
			contentTypes: nil,
			excludeContentTypes: nil,
			excludeSenderInboxIds: nil,
			sortBy: nil,
			insertedAfterNs: nil,
			insertedBeforeNs: nil
		)

		if let beforeNs {
			options.sentBeforeNs = beforeNs
		}

		if let afterNs {
			options.sentAfterNs = afterNs
		}

		if let limit {
			options.limit = Int64(limit)
		}

		let status: FfiDeliveryStatus? = switch deliveryStatus {
		case .published:
			FfiDeliveryStatus.published
		case .unpublished:
			FfiDeliveryStatus.unpublished
		case .failed:
			FfiDeliveryStatus.failed
		default:
			nil
		}

		options.deliveryStatus = status

		let direction: FfiDirection? = switch direction {
		case .ascending:
			FfiDirection.ascending
		default:
			FfiDirection.descending
		}

		options.direction = direction
		options.excludeContentTypes = excludeContentTypes
		options.excludeSenderInboxIds = excludeSenderInboxIds
		options.sortBy = sortBy?.toFfi()
		options.insertedAfterNs = insertedAfterNs
		options.insertedBeforeNs = insertedBeforeNs

		return try await ffiConversation.findEnrichedMessages(opts: options).compactMap {
			ffiDecodedMessage in
			DecodedMessageV2(ffiMessage: ffiDecodedMessage)
		}
	}

	/// Returns the HMAC keys for this DM, used for push notification decryption.
	///
	/// - Returns: A protobuf response containing HMAC key data keyed by topic.
	/// - Throws: If the keys cannot be retrieved.
	public func getHmacKeys() throws
		-> Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse
	{
		var hmacKeysResponse =
			Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse()
		let conversations: [Data: [FfiHmacKey]] = try ffiConversation.getHmacKeys()
		for convo in conversations {
			var hmacKeys =
				Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse.HmacKeys()
			for key in convo.value {
				var hmacKeyData =
					Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse
						.HmacKeyData()
				hmacKeyData.hmacKey = key.key
				hmacKeyData.thirtyDayPeriodsSinceEpoch = Int32(key.epoch)
				hmacKeys.values.append(hmacKeyData)
			}
			hmacKeysResponse.hmacKeys[
				Topic.groupMessage(convo.key.toHex).description
			] = hmacKeys
		}

		return hmacKeysResponse
	}

	/// Returns the topic strings to subscribe to for push notifications in this DM.
	///
	/// The returned array includes topics for this DM as well as any duplicate DM
	/// topics, ensuring push notifications are received regardless of which topic
	/// the server sends on.
	///
	/// - Returns: An array of topic strings for push notification registration.
	/// - Throws: If duplicate DMs cannot be queried.
	public func getPushTopics() async throws -> [String] {
		var duplicates = try await ffiConversation.findDuplicateDms()
		var topicIds = duplicates.map { $0.id().toHex }
		topicIds.append(id)
		return topicIds.map { Topic.groupMessage($0).description }
	}

	/// Returns debug information about this DM's internal state.
	///
	/// Intended for diagnostic purposes. The returned object contains details about
	/// the underlying MLS group state, epoch, and members.
	///
	/// - Returns: A ``ConversationDebugInfo`` snapshot of the conversation's internal state.
	/// - Throws: If the debug information cannot be retrieved.
	public func getDebugInformation() async throws -> ConversationDebugInfo {
		try await ConversationDebugInfo(
			ffiConversationDebugInfo: ffiConversation.conversationDebugInfo()
		)
	}

	/// Returns the last read timestamps for each member of this DM.
	///
	/// - Returns: A dictionary mapping inbox IDs to their last read time in nanoseconds since the Unix epoch.
	/// - Throws: If the read times cannot be retrieved.
	public func getLastReadTimes() throws -> [String: Int64] {
		try ffiConversation.getLastReadTimes()
	}

	/// Delete a message by its ID.
	/// - Parameter messageId: The hex-encoded message ID to delete.
	/// - Returns: The hex-encoded ID of the deletion message.
	/// - Throws: An error if the deletion fails (e.g., unauthorized deletion).
	public func deleteMessage(messageId: String) async throws -> String {
		try await ffiConversation.deleteMessage(messageId: messageId.hexToData).toHex
	}
}
