//
//  Conversation.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import Foundation
import XMTPProto

public enum Conversation {
	// TODO: It'd be nice to not have to expose these types as public, maybe we make this a struct with an enum prop instead of just an enum
	case v1(ConversationV1), v2(ConversationV2)

	public var peerAddress: String {
		switch self {
		case let .v1(conversationV1):
			return conversationV1.peerAddress
		case let .v2(conversationV2):
			return conversationV2.peerAddress
		}
	}

	public func send(text: String) async throws {
		switch self {
		case let .v1(conversationV1):
			try await conversationV1.send(content: text)
		case let .v2(conversationV2):
			try await conversationV2.send(content: text)
		}
	}

	public func messages() async throws -> [DecodedMessage] {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.messages()
		case let .v2(conversationV2):
			return try await conversationV2.messages()
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
