//
//  ConversationExport.swift
//
//
//  Created by Pat Nakajima on 2/1/23.
//

import XMTPProto

enum ConversationImportError: Error {
	case invalidData
}

struct ConversationV1Export: Codable {
	var version: String
	var peerAddress: String
	var createdAt: String
}

// TODO: Make these match ConversationContainer
struct ConversationV2Export: Codable {
	var version: String
	var topic: String
	var keyMaterial: String
	var peerAddress: String
	var createdAt: String
	var context: ConversationV2ContextExport?
}

struct ConversationV2ContextExport: Codable {
	var conversationId: String
	var metadata: [String: String]
}
