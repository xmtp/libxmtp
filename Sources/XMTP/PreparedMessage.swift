//
//  PreparedMessage.swift
//
//
//  Created by Pat Nakajima on 3/9/23.
//

import CryptoKit
import Foundation

public struct PreparedMessage {
	var messageEnvelope: Envelope
	var conversation: Conversation
	var onSend: () async throws -> Void

	public func decodedMessage() throws -> DecodedMessage {
		return try conversation.decode(messageEnvelope)
	}

	public func send() async throws {
		try await onSend()
	}

	var messageID: String {
		Data(SHA256.hash(data: messageEnvelope.message)).toHex
	}
}
