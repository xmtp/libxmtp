import Foundation
import LibXMTP

public struct Dm: Identifiable, Equatable, Hashable {
	var ffiConversation: FfiConversation
	var ffiLastMessage: FfiMessage? = nil
	var client: Client
	let streamHolder = StreamHolder()

    public enum ConversationError: Error, CustomStringConvertible, LocalizedError {
        case missingPeerInboxId
        
        public var description: String {
            switch self {
            case .missingPeerInboxId:
                return "ConversationError.missingPeerInboxId: The direct message is missing a peer inbox ID"
            }
        }
        
        public var errorDescription: String? {
            return description
        }
    }
    
	public var id: String {
		ffiConversation.id().toHex
	}

	public var topic: String {
		Topic.groupMessage(id).description
	}

	public var disappearingMessageSettings: DisappearingMessageSettings? {
		return try? {
			guard try isDisappearingMessagesEnabled() else { return nil }
			return try ffiConversation.conversationMessageDisappearingSettings()
				.map { DisappearingMessageSettings.createFromFfi($0) }
		}()
	}

	public func isDisappearingMessagesEnabled() throws -> Bool {
		return try ffiConversation.isConversationMessageDisappearingEnabled()
	}

	func metadata() async throws -> FfiConversationMetadata {
		return try await ffiConversation.groupMetadata()
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
		return try await metadata().creatorInboxId() == client.inboxID
	}
	
	public func isActive() throws -> Bool {
		return try ffiConversation.isActive()
	}

	public func creatorInboxId() async throws -> InboxId {
		return try await metadata().creatorInboxId()
	}

	public func addedByInboxId() throws -> InboxId {
		return try ffiConversation.addedByInboxId()
	}

	public var members: [Member] {
		get async throws {
			return try await ffiConversation.listMembers().map {
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

	public func updateConsentState(state: ConsentState) async throws {
		try ffiConversation.updateConsentState(state: state.toFFI)
	}

	public func consentState() throws -> ConsentState {
		return try ffiConversation.consentState().fromFFI
	}

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
					settings: ffiSettings)
		} else {
			try await clearDisappearingMessageSettings()
		}
	}

	public func clearDisappearingMessageSettings() async throws {
		try await ffiConversation.removeConversationMessageDisappearingSettings()
	}
    
    // Returns null if dm is not paused, otherwise the min version required to unpause this dm
    public func pausedForVersion() throws -> String? {
        return try ffiConversation.pausedForVersion()
    }

	public func processMessage(messageBytes: Data) async throws -> DecodedMessage? {
		let message =
			try await ffiConversation.processStreamedConversationMessage(
				envelopeBytes: messageBytes)
		return DecodedMessage.create(ffiMessage: message)
	}

	public func send<T>(content: T, options: SendOptions? = nil) async throws
		-> String
	{
		let encodeContent = try await encodeContent(
			content: content, options: options)
		return try await send(encodedContent: encodeContent)
	}

	public func send(encodedContent: EncodedContent) async throws -> String {
		let messageId = try await ffiConversation.send(
			contentBytes: encodedContent.serializedData())
		return messageId.toHex
	}

	public func encodeContent<T>(content: T, options: SendOptions?) async throws
		-> EncodedContent
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

		return encoded
	}

	public func prepareMessage(encodedContent: EncodedContent) async throws
		-> String
	{
		let messageId = try ffiConversation.sendOptimistic(
			contentBytes: encodedContent.serializedData())
		return messageId.toHex
	}

	public func prepareMessage<T>(content: T, options: SendOptions? = nil)
		async throws -> String
	{
		let encodeContent = try await encodeContent(
			content: content, options: options)
		return try ffiConversation.sendOptimistic(
			contentBytes: try encodeContent.serializedData()
		).toHex
	}

	public func publishMessages() async throws {
		try await ffiConversation.publishMessages()
	}

	public func endStream() {
		self.streamHolder.stream?.end()
	}

	public func streamMessages(onClose: (() -> Void)? = nil) -> AsyncThrowingStream<DecodedMessage, Error> {
		AsyncThrowingStream { continuation in
			let task = Task.detached {
				self.streamHolder.stream = await self.ffiConversation.stream(
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

				continuation.onTermination = { @Sendable reason in
					self.streamHolder.stream?.end()
				}
			}

			continuation.onTermination = { @Sendable reason in
				task.cancel()
				self.streamHolder.stream?.end()
			}
		}
	}

	public func lastMessage() async throws -> DecodedMessage? {
		if let ffiMessage = ffiLastMessage {
			return DecodedMessage.create(ffiMessage: ffiMessage)
		} else {
			return try await messages(limit: 1).first
		}
	}

	public func messages(
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		limit: Int? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all
	) async throws -> [DecodedMessage] {
		var options = FfiListMessagesOptions(
			sentBeforeNs: nil,
			sentAfterNs: nil,
			limit: nil,
			deliveryStatus: nil,
			direction: nil,
			contentTypes: nil
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

		let status: FfiDeliveryStatus? = {
			switch deliveryStatus {
			case .published:
				return FfiDeliveryStatus.published
			case .unpublished:
				return FfiDeliveryStatus.unpublished
			case .failed:
				return FfiDeliveryStatus.failed
			default:
				return nil
			}
		}()

		options.deliveryStatus = status

		let direction: FfiDirection? = {
			switch direction {
			case .ascending:
				return FfiDirection.ascending
			default:
				return FfiDirection.descending
			}
		}()

		options.direction = direction

		return try await ffiConversation.findMessages(opts: options).compactMap
		{
			ffiMessage in
			return DecodedMessage.create(ffiMessage: ffiMessage)
		}
	}

	public func messagesWithReactions(
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		limit: Int? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all
	) async throws -> [DecodedMessage] {
		var options = FfiListMessagesOptions(
			sentBeforeNs: nil,
			sentAfterNs: nil,
			limit: nil,
			deliveryStatus: nil,
			direction: nil,
			contentTypes: nil
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

		let status: FfiDeliveryStatus? = {
			switch deliveryStatus {
			case .published:
				return FfiDeliveryStatus.published
			case .unpublished:
				return FfiDeliveryStatus.unpublished
			case .failed:
				return FfiDeliveryStatus.failed
			default:
				return nil
			}
		}()

		options.deliveryStatus = status

		let direction: FfiDirection? = {
			switch direction {
			case .ascending:
				return FfiDirection.ascending
			default:
				return FfiDirection.descending
			}
		}()

		options.direction = direction

		return try await ffiConversation.findMessagesWithReactions(
			opts: options
		).compactMap {
			ffiMessageWithReactions in
			return DecodedMessage.create(ffiMessage: ffiMessageWithReactions)
		}
	}

    public func getHmacKeys() throws
    -> Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse {
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
				Topic.groupMessage(convo.key.toHex).description] = hmacKeys
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
        return ConversationDebugInfo(ffiConversationDebugInfo: try await ffiConversation.conversationDebugInfo())
    }
}
