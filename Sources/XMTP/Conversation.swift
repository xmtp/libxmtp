//
//  Conversation.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import Foundation
import XMTPProto

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
public enum Conversation {
	// TODO: It'd be nice to not have to expose these types as public, maybe we make this a struct with an enum prop instead of just an enum
	case v1(ConversationV1), v2(ConversationV2)

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

	public func decode(_ envelope: Envelope) throws -> DecodedMessage {
		switch self {
		case let .v1(conversationV1):
			return try conversationV1.decode(envelope: envelope)
		case let .v2(conversationV2):
			return try conversationV2.decode(envelope: envelope)
		}
	}

	public func send<T>(content: T, options: SendOptions? = nil, fallback _: String? = nil) async throws {
		switch self {
		case let .v1(conversationV1):
			try await conversationV1.send(content: content, options: options)
		case let .v2(conversationV2):
			try await conversationV2.send(content: content, options: options)
		}
	}

	/// Send a message to the conversation
	public func send(text: String, options: SendOptions? = nil) async throws {
		switch self {
		case let .v1(conversationV1):
			try await conversationV1.send(content: text, options: options)
		case let .v2(conversationV2):
			try await conversationV2.send(content: text, options: options)
		}
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
	public func messages(limit: Int? = nil, before: Date? = nil, after: Date? = nil) async throws -> [DecodedMessage] {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.messages(limit: limit, before: before, after: after)
		case let .v2(conversationV2):
			return try await conversationV2.messages(limit: limit, before: before, after: after)
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
