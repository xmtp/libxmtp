import Foundation
import LibXMTP

public struct Dm: Identifiable, Equatable, Hashable {
	var ffiConversation: FfiConversation
	var ffiLastMessage: FfiMessage? = nil
	var client: Client
	let streamHolder = StreamHolder()

	public var id: String {
		ffiConversation.id().toHex
	}

	public var topic: String {
		Topic.groupMessage(id).description
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

	public func creatorInboxId() async throws -> String {
		return try await metadata().creatorInboxId()
	}

	public func addedByInboxId() throws -> String {
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

	public var peerInboxId: String {
		get throws {
			try ffiConversation.dmPeerInboxId()
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

	public func processMessage(messageBytes: Data) async throws -> Message? {
		let message =
			try await ffiConversation.processStreamedConversationMessage(
				envelopeBytes: messageBytes)
		return Message.create(client: client, ffiMessage: message)
	}

	public func send<T>(content: T, options: SendOptions? = nil) async throws
		-> String
	{
		let encodeContent = try await encodeContent(
			content: content, options: options)
		return try await send(encodedContent: encodeContent)
	}

	public func send(encodedContent: EncodedContent) async throws -> String {
		if try consentState() == .unknown {
			try await updateConsentState(state: .allowed)
		}

		let messageId = try await ffiConversation.send(
			contentBytes: encodedContent.serializedData())
		return messageId.toHex
	}

	public func encodeContent<T>(content: T, options: SendOptions?) async throws
		-> EncodedContent
	{
		let codec = client.codecRegistry.find(for: options?.contentType)

		func encode<Codec: ContentCodec>(codec: Codec, content: Any) throws
			-> EncodedContent
		{
			if let content = content as? Codec.T {
				return try codec.encode(content: content, client: client)
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
		if try consentState() == .unknown {
			try await updateConsentState(state: .allowed)
		}

		let messageId = try ffiConversation.sendOptimistic(
			contentBytes: encodedContent.serializedData())
		return messageId.toHex
	}

	public func prepareMessage<T>(content: T, options: SendOptions? = nil)
		async throws -> String
	{
		if try consentState() == .unknown {
			try await updateConsentState(state: .allowed)
		}

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

	public func streamMessages() -> AsyncThrowingStream<Message, Error> {
		AsyncThrowingStream { continuation in
			let task = Task.detached {
				self.streamHolder.stream = await self.ffiConversation.stream(
					messageCallback: MessageCallback(client: self.client) {
						message in
						guard !Task.isCancelled else {
							continuation.finish()
							return
						}
						if let message = Message.create(
							client: self.client, ffiMessage: message)
						{
							continuation.yield(message)
						}
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

	public func lastMessage() async throws -> Message? {
		if let ffiMessage = ffiLastMessage {
			return Message.create(client: self.client, ffiMessage: ffiMessage)
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
	) async throws -> [Message] {
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
			return Message.create(client: self.client, ffiMessage: ffiMessage)
		}
	}
}
