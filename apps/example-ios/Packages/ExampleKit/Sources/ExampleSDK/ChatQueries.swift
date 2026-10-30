import ExampleCore
import Foundation
import XmtpSdk

func coreFile(_ file: RemoteAttachment) -> FileDescriptor {
	FileDescriptor(
		url: file.url,
		digest: file.contentDigest,
		secret: file.secret,
		salt: file.salt,
		nonce: file.nonce,
		scheme: file.scheme,
		length: file.contentLength,
		filename: file.filename,
	)
}

func sdkFile(_ file: FileDescriptor) -> RemoteAttachment {
	RemoteAttachment(
		url: file.url,
		contentDigest: file.digest,
		secret: file.secret,
		salt: file.salt,
		nonce: file.nonce,
		scheme: file.scheme,
		contentLength: file.length,
		filename: file.filename,
	)
}

func coreBody(_ body: MessageBody, fallback: String?) -> String {
	switch body {
	case let .text(text),
	     let .markdown(text): text
	case .deletedMessage: "Message deleted"
	default: fallback ??
		"Unsupported reply"
	}
}

func coreContent(_ content: SDKMessageContent, fallback: String?, quote: String?) -> ChatContent {
	switch content {
	case let .standard(value):
		switch value {
		case let .text(text): .text(text)
		case let .markdown(text): .markdown(text)
		case let .reply(_, body): .reply(quote: quote ?? "Reply", body: coreBody(body, fallback: fallback))
		case let .remoteAttachment(file): .attachment(coreFile(file))
		case .deletedMessage: .deleted
		case .reaction: .reaction
		default: .unsupported(fallback ?? "Unsupported content")
		}
	case .custom, .unknown: .unsupported(fallback ?? "Unsupported content")
	}
}

func coreMessage(_ message: Message) -> ChatMessage {
	let quote = message.inReplyTo.map { coreBody($0.content, fallback: $0.fallback) }
	return ChatMessage(
		id: message.id,
		conversationID: message.conversationId,
		sender: message.senderInboxId,
		sentAt: message.sentAt.ns,
		insertedAt: message.insertedAt.ns,
		delivery: message.deliveryStatus == .published ? .published : message.deliveryStatus == .failed ? .failed : .queued,
		content: coreContent(message.content, fallback: message.fallback, quote: quote),
		position: message.deliveryCursor.map { HistoryPosition(sentAt: message.sentAt.ns, cursor: $0) },
		reactions: message.reactions.map { ReactionRecord(
			sender: $0.senderInboxId,
			emoji: $0.reaction.content,
			schema: String(describing: $0.reaction.schema),
			added: $0.reaction.action == .added,
			sentAt: $0.sentAt.ns,
		) },
	)
}

func historyPosition(_ value: HistoryPosition?) -> MessageHistoryPosition? {
	value.map { MessageHistoryPosition(
		sentAt: Timestamp(ns: $0.sentAt),
		deliveryCursor: $0.cursor,
	) }
}

func corePosition(_ value: MessageHistoryPosition?) -> HistoryPosition? {
	value.map { HistoryPosition(
		sentAt: $0.sentAt.ns,
		cursor: $0.deliveryCursor,
	) }
}

func enrichGroupMemberCount(_ record: ConversationRecord,
                            lookup: @Sendable () async throws -> Int) async throws -> ConversationRecord
{
	var result = record
	do {
		result.memberCount = try await lookup()
	} catch is CancellationError {
		throw CancellationError()
	} catch {
		try Task.checkCancellation()
	}
	return result
}

public final class SDKChatQueries: ChatQueries, Sendable {
	private let client: SDKClient
	private let limiter: AsyncLimiter
	public init(client: SDKClient, limiter: AsyncLimiter) {
		self.client = client
		self.limiter = limiter
	}

	private func get(_ id: String) async throws -> Conversation {
		guard let value = try await client.conversations
			.getById(id: id) else { throw ChatError.unavailable }
		return value
	}

	public func conversation(id: String) async throws -> ConversationRecord {
		let value = try await get(id)
		var record: ConversationRecord
		switch value {
		case let .group(group): let state = try await limiter
			.withPermit { try await group.state() }
			record = ConversationRecord(
				id: id,
				kind: .group,
				name: state.name,
				consent: Self.consent(state.common.consentState),
			)
			record = try await enrichGroupMemberCount(record) { [limiter] in
				try await limiter.withPermit { try await value.members().count }
			}
		case let .dm(dm): let state = try await limiter.withPermit { try await dm.state() }
			let peer = try await limiter
				.withPermit { try await dm.peerInboxId() }
			record = ConversationRecord(
				id: id,
				kind: .dm,
				peerInboxID: peer,
				consent: Self.consent(state.consentState),
			)
			if let peer {
				record = try await enrichPeerHandle(record) { [limiter, client] in
					try await limiter.withPermit { try await client.inboxStates(
						ids: [peer],
						refreshFromNetwork: false,
					).first?.handle }
				}
			}
		}
		return record
	}

	private static func consent(_ state: ConsentState)
		-> ConversationConsent
	{
		switch state {
		case .allowed: .allowed
		case .denied: .denied
		case .unknown: .unknown
		}
	}

	public func history(id: String, ascending: Bool, before: HistoryPosition?,
	                    after: HistoryPosition?) async throws -> ChatPage
	{
		let value = try await get(id)
		let page = try await limiter.withPermit { try await value.messageHistoryPage(
			options: historyMessageOptions(ascending: ascending),
			before: historyPosition(before),
			after: historyPosition(after),
		) }
		return ChatPage(
			rows: page.messages.map(coreMessage),
			first: corePosition(page.firstPosition),
			last: corePosition(page.lastPosition),
			hasMore: page.hasMore,
			skipped: page.skippedCount,
		)
	}

	public func pending(id: String, before: RecoveryPosition?) async throws -> PendingPage {
		let value = try await get(id)
		let page = try await limiter.withPermit { try await value.messageRecoveryPage(
			options: pendingMessageOptions(),
			before: before.map { MessageRecoveryPosition(sentAt: Timestamp(ns: $0.sentAt), messageCursor: $0.id) },
			after: nil,
		) }
		return PendingPage(
			rows: page.messages.map(coreMessage),
			last: page.lastPosition.map { RecoveryPosition(sentAt: $0.sentAt.ns, id: $0.messageCursor) },
			hasMore: page.hasMore,
			skipped: page.skippedCount,
		)
	}

	public func message(id: String) async throws -> ChatMessage? {
		try await client.conversations.getMessageById(id: id)
			.map(coreMessage)
	}

	public func queue(conversationID: String, content: OutboundContent) async throws -> String {
		let value = try await get(conversationID)
		let options = SendOptions(optimistic: true)
		switch content {
		case let .text(text): return try await value.sendText(text: text, options: options)
		case let .reply(parent, text): guard let message = try await client.conversations.getMessageById(id: parent),
		                                     message.conversationId == conversationID
			else { throw ChatError.unavailable }
			return try await message.reply(
				text,
				options: options,
			)
		case let .reaction(parent, emoji, remove): guard let message = try await client.conversations
			.getMessageById(id: parent),
			message.conversationId == conversationID else { throw ChatError.unavailable }
			return try await message.react(
				Reaction(content: emoji, action: remove ? .removed : .added, schema: .unicode),
				options: options,
			)
		case let .attachment(file): return try await value.sendRemoteAttachment(attachment: sdkFile(file), options: options)
		}
	}

	public func publish(conversationID: String, messageID: String) async throws {
		try await get(conversationID).publishMessage(id: messageID)
	}

	/// Acceptance only. The Core coordinator persists and publishes the returned deletion ID.
	public func delete(messageID: String) async throws -> String {
		guard let message = try await client.conversations.getMessageById(id: messageID),
		      message.senderInboxId == client.inboxId(),
		      message.deliveryStatus == .published else { throw ChatError.rejected }
		return try await message.delete()
	}

	public func media(conversationID: String) async throws
		-> [ChatMessage]
	{
		let value = try await get(conversationID)
		return try await limiter
			.withPermit { try await value.messages(options: ListMessagesOptions(
				limit: 50,
				direction: .descending,
				contentTypes: [standardContentType(kind: .remoteAttachment)],
			)) }.map(coreMessage)
	}
}

func pendingMessageOptions() -> ListMessagesOptions {
	ListMessagesOptions(limit: 50, direction: .descending, sortBy: .sentAt, kind: .application)
}

func historyMessageOptions(ascending: Bool) -> ListMessagesOptions {
	ListMessagesOptions(
		limit: 50,
		direction: ascending ? .ascending : .descending,
		sortBy: .sentAt,
		deliveryStatus: .published,
		kind: .application,
	)
}
