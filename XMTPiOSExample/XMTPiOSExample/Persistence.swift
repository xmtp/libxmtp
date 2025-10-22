//
//  Persistence.swift
//
//
//  Created by Pat Nakajima on 1/20/23.
//

import Foundation
import KeychainAccess
import XMTPiOS

struct Persistence {
	var keychain: Keychain

	init() {
		keychain = Keychain(service: "com.xmtp.XMTPiOSExample")
	}

	func saveKeys(_ keys: Data) {
		keychain[data: "keys"] = keys
	}

	func loadKeys() -> Data? {
		do {
			return try keychain.getData("keys")
		} catch {
			print("Error loading keys data: \(error)")
			return nil
		}
	}

	func saveAddress(_ address: String) {
		keychain[string: "address"] = address
	}

	func loadAddress() -> String? {
		do {
			return try keychain.getString("address")
		} catch {
			print("Error loading address data: \(error)")
			return nil
		}
	}

//	func load(conversationTopic: String) throws -> ConversationContainer? {
//		guard let data = try keychain.getData(key(topic: conversationTopic)) else {
//			return nil
//		}
//
//		let decoder = JSONDecoder()
//		let decoded = try decoder.decode(ConversationContainer.self, from: data)
//
//		return decoded
//	}

	func save(conversation _: Conversation) throws {
//		keychain[data: key(topic: conversation.topic)] = try JSONEncoder().encode(conversation.encodedContainer)
	}

	func key(topic: String) -> String {
		"conversation-\(topic)"
	}
}
