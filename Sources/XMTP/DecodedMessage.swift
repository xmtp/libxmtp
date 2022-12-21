//
//  DecodedMessage.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import Foundation

/// Decrypted messages from a conversation.
public struct DecodedMessage {
	/// The text of a message
	public var body: String

	/// The wallet address of the sender of the message
	public var senderAddress: String

	/// When the message was sent
	public var sent: Date

	public init(body: String, senderAddress: String, sent: Date) {
		self.body = body
		self.senderAddress = senderAddress
		self.sent = sent
	}
}
