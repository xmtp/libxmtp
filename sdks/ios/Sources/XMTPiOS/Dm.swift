import Foundation

public struct Dm: Identifiable, Equatable, Hashable {
	var ffiConversation: FfiConversation
	var ffiLastMessage: FfiMessage?
	var ffiCommitLogForkStatus: Bool?
	var client: Client
	let streamHolder = StreamHolder()

	public enum ConversationError: Error, CustomStringConvertible, LocalizedError {
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

	public var id: String {
		ffiConversation.id().toHex
	}

	public var topic: String {
		Topic.groupMessage(id).description
	}

	public var disappearingMessageSettings: DisappearingMessageSettings? {
		try? {
			guard try isDisappearingMessagesEnabled() else { return nil }
			return try ffiConversation.conversationMessageDisappearingSettings()
				.map { DisappearingMessageSettings.createFromFfi($0) }
		}()
	}

	public func isDisappearingMessagesEnabled() throws -> Bool {
		try ffiConversation.isConversationMessageDisappearingEnabled()
	}

	func metadata() async throws -> FfiConversationMetadata {
		try await ffiConversation.groupMetadata()
	}

	public func sync() async throws {
		try await ffiConversation.sync()
	}

	public static func == (lhs: Dm, rhs: Dm) -> Bool {
		lhs.id == rhs.id
	}

	public func hash(into hasher: inout Hasher) {
		id.hash(into: &hasher)
	}

	public func isCreator() async throws -> Bool {
		try await metadata().creatorInboxId() == client.inboxID
	}

	public func isActive() throws -> Bool {
		try ffiConversation.isActive()
	}

	public func creatorInboxId() async throws -> InboxId {
		try await metadata().creatorInboxId()
	}

	public func addedByInboxId() throws -> InboxId {
		try ffiConversation.addedByInboxId()
	}

	public var members: [Member] {
		get async throws {
			try await ffiConversation.listMembers().map {
				ffiGroupMember in
				Member(ffiGroupMember: ffiGroupMember)
			}
		}
	}

	public var peerInboxId: InboxId {
		get throws {
			guard let inboxId = ffiConversation.dmPeerInboxId() else {
				throw ConversationError.missingPeerInboxId
			}
			return inboxId
		}
	}

	public var createdAt: Date {
		Date(millisecondsSinceEpoch: ffiConversation.createdAtNs())
	}

	public var createdAtNs: Int64 {
		ffiConversation.createdAtNs()
	}

	public var lastActivityAtNs: Int64 {
		ffiLastMessage?.sentAtNs ?? createdAtNs
	}

	public func updateConsentState(state: ConsentState) async throws {
		try ffiConversation.updateConsentState(state: state.toFFI)
	}

	public func consentState() throws -> ConsentState {
		try ffiConversation.consentState().fromFFI
	}

	public func updateDisappearingMessageSettings(
		_ disappearingMessageSettings: DisappearingMessageSettings?,
	) async throws {
		if let settings = disappearingMessageSettings {
			let ffiSettings = FfiMessageDisappearingSettings(
				fromNs: settings.disappearStartingAtNs,
				inNs: settings.retentionDurationInNs,
			)
			try await ffiConversation
				.updateConversationMessageDisappearingSettings(
					settings: ffiSettings,
				)
		} else {
			try await clearDisappearingMessageSettings()
		}
	}

	public func clearDisappearingMessageSettings() async throws {
		try await ffiConversation.removeConversationMessageDisappearingSettings()
	}

	/// Returns null if dm is not paused, otherwise the min version required to unpause this dm
	public func pausedForVersion() throws -> String? {
		try ffiConversation.pausedForVersion()
	}

	public func processMessage(messageBytes: Data) async throws -> DecodedMessage? {
		let messages =
			try await ffiConversation.processStreamedConversationMessage(
				envelopeBytes: messageBytes,
			)
		guard let firstMessage = messages.first else {
			return nil
		}
		return DecodedMessage.create(ffiMessage: firstMessage)
	}

	public func send(content: some Any, options: SendOptions? = nil) async throws
		-> String
	{
		let (encodeContent, visibilityOptions) = try await encodeContent(
			content: content, options: options,
		)
		return try await send(encodedContent: encodeContent, visibilityOptions: visibilityOptions)
	}

	public func send(
		encodedContent: EncodedContent, visibilityOptions: MessageVisibilityOptions? = nil,
	) async throws -> String {
		let opts = visibilityOptions?.toFfi() ?? FfiSendMessageOpts(shouldPush: true)
		let messageId = try await ffiConversation.send(
			contentBytes: encodedContent.serializedData(),
			opts: opts,
		)
		return messageId.toHex
	}

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
			shouldPush: shouldPush(codec: codec, content: content),
		)

		return (encoded, visibilityOptions)
	}

	public func prepareMessage(
		encodedContent: EncodedContent,
		visibilityOptions: MessageVisibilityOptions? = nil,
		noSend: Bool = false,
	) async throws
		-> String
	{
		let shouldPush = visibilityOptions?.shouldPush ?? true
		let messageId: Data
		if noSend {
			messageId = try ffiConversation.prepareMessage(
				contentBytes: encodedContent.serializedData(),
				shouldPush: shouldPush,
			)
		} else {
			let opts = visibilityOptions?.toFfi() ?? FfiSendMessageOpts(shouldPush: true)
			messageId = try ffiConversation.sendOptimistic(
				contentBytes: encodedContent.serializedData(),
				opts: opts,
			)
		}
		return messageId.toHex
	}

	public func prepareMessage(content: some Any, options: SendOptions? = nil, noSend: Bool = false)
		async throws -> String
	{
		let (encodeContent, visibilityOptions) = try await encodeContent(
			content: content, options: options,
		)
		return try await prepareMessage(
			encodedContent: encodeContent,
			visibilityOptions: visibilityOptions,
			noSend: noSend,
		)
	}

	public func publishMessages() async throws {
		try await ffiConversation.publishMessages()
	}

	public func publishMessage(messageId: String) async throws {
		try await ffiConversation.publishStoredMessage(messageId: messageId.hexToData)
	}

	public func endStream() {
		streamHolder.stream?.end()
	}

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
					},
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

	public func lastMessage() async throws -> DecodedMessage? {
		if let ffiMessage = ffiLastMessage {
			DecodedMessage.create(ffiMessage: ffiMessage)
		} else {
			try await messages(limit: 1).first
		}
	}

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
		insertedBeforeNs: Int64? = nil,
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
			insertedBeforeNs: nil,
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
		insertedBeforeNs: Int64? = nil,
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
			insertedBeforeNs: nil,
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
			opts: options,
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
		insertedBeforeNs: Int64? = nil,
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
				insertedBeforeNs: insertedBeforeNs,
			),
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
		insertedBeforeNs: Int64? = nil,
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
			insertedBeforeNs: nil,
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
				Topic.groupMessage(convo.key.toHex).description,
			] = hmacKeys
		}

		return hmacKeysResponse
	}

	public func getPushTopics() async throws -> [String] {
		var duplicates = try await ffiConversation.findDuplicateDms()
		var topicIds = duplicates.map { $0.id().toHex }
		topicIds.append(id)
		return topicIds.map { Topic.groupMessage($0).description }
	}

	public func getDebugInformation() async throws -> ConversationDebugInfo {
		try await ConversationDebugInfo(
			ffiConversationDebugInfo: ffiConversation.conversationDebugInfo(),
		)
	}

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
