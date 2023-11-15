//
//  Conversation.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import Foundation

public enum ConversationContainer: Codable {
	case v1(ConversationV1Container), v2(ConversationV2Container)

	public func decode(with client: Client) -> Conversation {
		switch self {
		case let .v1(container):
			return .v1(container.decode(with: client))
		case let .v2(container):
			return .v2(container.decode(with: client))
		}
	}
}

/// Wrapper that provides a common interface between ``ConversationV1`` and ``ConversationV2`` objects.
public enum Conversation: Sendable {
	// TODO: It'd be nice to not have to expose these types as public, maybe we make this a struct with an enum prop instead of just an enum
	case v1(ConversationV1), v2(ConversationV2)

	public enum Version {
		case v1, v2
	}

	public func consentState() async -> ConsentState {
		let client: Client

		switch self {
		case .v1(let conversationV1):
			client = conversationV1.client
		case .v2(let conversationV2):
			client = conversationV2.client
		}

		return await client.contacts.consentList.state(address: peerAddress)
	}

	public var version: Version {
		switch self {
		case .v1:
			return .v1
		case .v2:
			return .v2
		}
	}

	public var createdAt: Date {
		switch self {
		case let .v1(conversationV1):
			return conversationV1.sentAt
		case let .v2(conversationV2):
			return conversationV2.createdAt
		}
	}

	public var encodedContainer: ConversationContainer {
		switch self {
		case let .v1(conversationV1):
			return .v1(conversationV1.encodedContainer)
		case let .v2(conversationV2):
			return .v2(conversationV2.encodedContainer)
		}
	}

	/// The wallet address of the other person in this conversation.
	public var peerAddress: String {
		switch self {
		case let .v1(conversationV1):
			return conversationV1.peerAddress
		case let .v2(conversationV2):
			return conversationV2.peerAddress
		}
	}

	/// An optional string that can specify a different context for a conversation with another account address.
	///
	/// > Note: ``conversationID`` is only available for ``ConversationV2`` conversations.
	public var conversationID: String? {
		switch self {
		case .v1:
			return nil
		case let .v2(conversation):
			return conversation.context.conversationID
		}
	}

	/// Exports the serializable topic data required for later import.
	/// See Conversations.importTopicData()
	public func toTopicData() -> Xmtp_KeystoreApi_V1_TopicMap.TopicData {
		Xmtp_KeystoreApi_V1_TopicMap.TopicData.with {
			$0.createdNs = UInt64(createdAt.timeIntervalSince1970 * 1000) * 1_000_000
			$0.peerAddress = peerAddress
			if case let .v2(cv2) = self {
				$0.invitation = Xmtp_MessageContents_InvitationV1.with {
					$0.topic = cv2.topic
					$0.context = cv2.context
					$0.aes256GcmHkdfSha256 = Xmtp_MessageContents_InvitationV1.Aes256gcmHkdfsha256.with {
						$0.keyMaterial = cv2.keyMaterial
					}
				}
			}
		}
	}

	public func decode(_ envelope: Envelope) throws -> DecodedMessage {
		switch self {
		case let .v1(conversationV1):
			return try conversationV1.decode(envelope: envelope)
		case let .v2(conversationV2):
			return try conversationV2.decode(envelope: envelope)
		}
	}

	public func encode<Codec: ContentCodec, T>(codec: Codec, content: T) async throws -> Data where Codec.T == T {
		switch self {
		case let .v1:
			throw RemoteAttachmentError.v1NotSupported
		case let .v2(conversationV2):
			return try await conversationV2.encode(codec: codec, content: content)
		}
	}

	public func prepareMessage<T>(content: T, options: SendOptions? = nil) async throws -> PreparedMessage {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.prepareMessage(content: content, options: options ?? .init())
		case let .v2(conversationV2):
			return try await conversationV2.prepareMessage(content: content, options: options ?? .init())
		}
	}

	// This is a convenience for invoking the underlying `client.publish(prepared.envelopes)`
	// If a caller has a `Client` handy, they may opt to do that directly instead.
	@discardableResult public func send(prepared: PreparedMessage) async throws -> String {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.send(prepared: prepared)
		case let .v2(conversationV2):
			return try await conversationV2.send(prepared: prepared)
		}
	}

	@discardableResult public func send<T>(content: T, options: SendOptions? = nil, fallback _: String? = nil) async throws -> String {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.send(content: content, options: options)
		case let .v2(conversationV2):
			return try await conversationV2.send(content: content, options: options)
		}
	}

	@discardableResult public func send(encodedContent: EncodedContent, options: SendOptions? = nil) async throws -> String {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.send(encodedContent: encodedContent, options: options)
		case let .v2(conversationV2):
			return try await conversationV2.send(encodedContent: encodedContent, options: options)
		}
	}

	/// Send a message to the conversation
	public func send(text: String, options: SendOptions? = nil) async throws -> String {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.send(content: text, options: options)
		case let .v2(conversationV2):
			return try await conversationV2.send(content: text, options: options)
		}
	}

	public var clientAddress: String {
		return client.address
	}

	/// The topic identifier for this conversation
	public var topic: String {
		switch self {
		case let .v1(conversation):
			return conversation.topic.description
		case let .v2(conversation):
			return conversation.topic
		}
	}

	public func streamEphemeral() -> AsyncThrowingStream<Envelope, Error>? {
		switch self {
		case let .v1(conversation):
			return conversation.streamEphemeral()
		case let .v2(conversation):
			return conversation.streamEphemeral()
		}
	}

	/// Returns a stream you can iterate through to receive new messages in this conversation.
	///
	/// > Note: All messages in the conversation are returned by this stream. If you want to filter out messages
	/// by a sender, you can check the ``Client`` address against the message's ``peerAddress``.
	public func streamMessages() -> AsyncThrowingStream<DecodedMessage, Error> {
		switch self {
		case let .v1(conversation):
			return conversation.streamMessages()
		case let .v2(conversation):
			return conversation.streamMessages()
		}
	}

	/// List messages in the conversation
	public func messages(limit: Int? = nil, before: Date? = nil, after: Date? = nil, direction: PagingInfoSortDirection? = .descending) async throws -> [DecodedMessage] {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.messages(limit: limit, before: before, after: after, direction: direction)
		case let .v2(conversationV2):
			return try await conversationV2.messages(limit: limit, before: before, after: after, direction: direction)
		}
	}

	public func decryptedMessages(limit: Int? = nil, before: Date? = nil, after: Date? = nil, direction: PagingInfoSortDirection? = .descending) async throws -> [DecryptedMessage] {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.decryptedMessages(limit: limit, before: before, after: after, direction: direction)
		case let .v2(conversationV2):
			return try await conversationV2.decryptedMessages(limit: limit, before: before, after: after, direction: direction)
		}
	}

	var client: Client {
		switch self {
		case let .v1(conversationV1):
			return conversationV1.client
		case let .v2(conversationV2):
			return conversationV2.client
		}
	}
}

extension Conversation: Hashable, Equatable {
	public static func == (lhs: Conversation, rhs: Conversation) -> Bool {
		lhs.topic == rhs.topic
	}

	public func hash(into hasher: inout Hasher) {
		hasher.combine(topic)
	}
}
