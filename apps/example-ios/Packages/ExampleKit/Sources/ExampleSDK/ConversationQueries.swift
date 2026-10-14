import ExampleCore
import Foundation
import XmtpSdk

func unreadOptions(marker: Int64, ownInboxID: String) -> ListMessagesOptions {
	ListMessagesOptions(
		insertedAfter: Timestamp(ns: marker),
		deliveryStatus: .published,
		contentTypes: [StandardContentKind.text, .markdown, .reply, .remoteAttachment].map { standardContentType(kind: $0) },
		excludeSenderInboxIds: [ownInboxID],
	)
}

func enrichPeerHandle(_ record: ConversationRecord,
                      lookup: @Sendable () async throws -> String?) async throws -> ConversationRecord
{
	var result = record
	do { result.peerHandle = try await lookup() } catch is CancellationError { throw CancellationError() } catch {
	/* Keep state when optional identity enrichment fails. */ }
	return result
}

func limitedConversationLookup<T: Sendable>(
	limiter: AsyncLimiter,
	lookup: @Sendable () async throws -> T?,
) async throws -> T? {
	try await limiter.withPermit(lookup)
}

public final class SDKConversationQueries: ConversationQueries, Sendable {
	private let client: SDKClient
	private let limiter: AsyncLimiter
	private let identityDirectory: IdentityDirectory?

	public init(
		client: SDKClient,
		limiter: AsyncLimiter,
		identityDirectory: IdentityDirectory? = nil,
	) {
		self.client = client
		self.limiter = limiter
		self.identityDirectory = identityDirectory
	}

	public func list(consent: ConversationConsent, limit: Int) async throws -> [ConversationRecord] {
		let values = try await limiter.withPermit {
			try await self.client.conversations.list(options: ListConversationsOptions(
				consentStates: [Self.sdkConsent(consent)], limit: UInt32(limit), orderBy: .lastActivity,
			))
		}
		return values.map { value in
			switch value {
			case .group: ConversationRecord(id: value.id(), kind: .group, consent: consent)
			case .dm: ConversationRecord(id: value.id(), kind: .dm, consent: consent)
			}
		}
	}

	public func details(_ record: ConversationRecord, marker: Int64) async throws -> ConversationRecord {
		guard let value = try await limitedConversationLookup(limiter: limiter, lookup: {
			try await self.client.conversations.getById(id: record.id)
		}) else { return record }
		var result = try await stateDetails(value, record: record)
		let own = client.inboxId()
		let unread = try await limiter.withPermit {
			try await value.countMessages(options: unreadOptions(marker: marker, ownInboxID: own))
		}
		result.unread = Int(clamping: unread)
		if let message = try await limiter.withPermit({ try await value.lastMessage() }) {
			result.preview = Self.preview(message)
			result.lastActivity = message.sentAt.date
		}
		return result
	}

	private func stateDetails(_ value: Conversation, record: ConversationRecord) async throws -> ConversationRecord {
		switch value {
		case let .group(group):
			let state = try await limiter.withPermit { try await group.state() }
			var result = record
			result.name = state.name
			result.consent = Self.coreConsent(state.common.consentState)
			return result
		case let .dm(dm):
			return try await dmDetails(dm, record: record)
		}
	}

	private func dmDetails(_ dm: Dm, record: ConversationRecord) async throws -> ConversationRecord {
		var result = record
		let state = try await limiter.withPermit { try await dm.state() }
		result.consent = Self.coreConsent(state.consentState)
		result.peerInboxID = try await limiter.withPermit { try await dm.peerInboxId() }
		guard let peer = result.peerInboxID else { return result }
		return try await enrichPeerHandle(result) { [identityDirectory, limiter, client] in
			if let identityDirectory {
				try await identityDirectory.load([peer])
				return await identityDirectory.identities[peer]?.handle
			}
			return try await limiter
				.withPermit { try await client.inboxStates(ids: [peer], refreshFromNetwork: false).first?.handle }
		}
	}

	private static func preview(_ message: Message) -> String {
		switch message.content {
		case let .standard(.text(text)), let .standard(.markdown(text)):
			text
		case let .standard(.reply(_, body)):
			switch body {
			case let .text(text), let .markdown(text):
				text
			default:
				message.fallback ?? "Reply"
			}
		case .standard(.remoteAttachment):
			"Attachment"
		default:
			message.fallback ?? ""
		}
	}

	public func setConsent(id: String, consent: ConversationConsent) async throws {
		guard let value = try await limitedConversationLookup(limiter: limiter, lookup: {
			try await self.client.conversations.getById(id: id)
		}) else { return }
		try await value.updateConsentState(state: Self.sdkConsent(consent))
	}

	private static func sdkConsent(_ value: ConversationConsent) -> ConsentState {
		switch value {
		case .allowed: .allowed
		case .unknown: .unknown
		case .denied: .denied
		}
	}

	private static func coreConsent(_ value: ConsentState) -> ConversationConsent {
		switch value {
		case .allowed: .allowed
		case .unknown: .unknown
		case .denied: .denied
		}
	}
}

let sessionEventKinds: [EventKind] = [.handleChanged, .consentChanged, .messageDeleted]

func sessionEvent(_ event: ClientEvent) -> SessionEvent? {
	switch event {
	case let .handleChanged(value):
		.handleChanged(inboxID: value.inboxId, handle: value.handle, otherHandles: [])
	case let .messageDeleted(value):
		.messageDeleted(
			conversationID: value.groupId.map { String(format: "%02x", $0) }.joined(),
			messageID: value.messageId.map { String(format: "%02x", $0) }.joined(),
			locallyDeleted: value.cause == .deletedLocally,
		)
	case .consentChanged, .lagged:
		.changed
	default:
		nil
	}
}

public extension SDKConversationQueries {
	func readers(
		projectMessage: @escaping @Sendable (String) async throws -> Void,
		projectConversation: @escaping @Sendable (String) async throws -> Void,
		projectEvent: @escaping @Sendable (SessionEvent) async throws -> Void,
		connection: @escaping @Sendable (String?) -> Void,
	) -> SessionReaders {
		SessionReaders(messages: {
			let options = MessageStreamOptions(onConnectionStateChange: { _, state in connection(Self.banner(state)) })
			let stream = try await self.client.conversations.streamAllMessages(options: options)
			for try await message in stream {
				try Task.checkCancellation()
				try await projectMessage(message.conversationId)
			}
		}, conversations: {
			let stream = try await self.client.conversations.stream()
			for try await conversation in stream {
				try Task.checkCancellation()
				try await projectConversation(conversation.id())
			}
		}, events: {
			let stream = try await self.client.events(EventFilter(kinds: sessionEventKinds))
			for try await event in stream {
				try Task.checkCancellation()
				if let projection = sessionEvent(event) {
					try await projectEvent(projection)
				}
			}
		})
	}

	private static func banner(_ state: ConnectionState) -> String? {
		switch state {
		case .connected: nil
		case .connecting: "Connecting to your server…"
		case .reconnecting: "Reconnecting to your server…"
		case .failed: "Cannot connect to your server."
		case .closed: "Not connected to your server."
		}
	}
}
