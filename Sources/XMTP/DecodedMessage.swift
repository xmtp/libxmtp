//
//  DecodedMessage.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import Foundation

/// Decrypted messages from a conversation.
public struct DecodedMessage {
	public var id: String = ""

	public var encodedContent: EncodedContent

	/// The wallet address of the sender of the message
	public var senderAddress: String

	/// When the message was sent
	public var sent: Date

	public init(encodedContent: EncodedContent, senderAddress: String, sent: Date) {
		self.encodedContent = encodedContent
		self.senderAddress = senderAddress
		self.sent = sent
	}

	public func content<T>() throws -> T {
		return try encodedContent.decoded()
	}

	var fallbackContent: String {
		encodedContent.fallback
	}

	var body: String {
		do {
			return try content()
		} catch {
			return fallbackContent
		}
	}
}

public extension DecodedMessage {
	static func preview(body: String, senderAddress: String, sent: Date) -> DecodedMessage {
		// swiftlint:disable force_try
		let encoded = try! TextCodec().encode(content: body)
		// swiftlint:enable force_try
		return DecodedMessage(encodedContent: encoded, senderAddress: senderAddress, sent: sent)
	}
}
