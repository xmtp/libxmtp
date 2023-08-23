//
//  DecodedMessage.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import Foundation

/// Decrypted messages from a conversation.
public struct DecodedMessage: Sendable {
    public var topic: String

	public var id: String = ""

	public var encodedContent: EncodedContent

	/// The wallet address of the sender of the message
	public var senderAddress: String

	/// When the message was sent
	public var sent: Date

    public init(topic: String, encodedContent: EncodedContent, senderAddress: String, sent: Date) {
        self.topic = topic
		self.encodedContent = encodedContent
		self.senderAddress = senderAddress
		self.sent = sent
	}

	public func content<T>() throws -> T {
		return try encodedContent.decoded()
	}

	public var fallbackContent: String {
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
    static func preview(topic: String, body: String, senderAddress: String, sent: Date) -> DecodedMessage {
		// swiftlint:disable force_try
		let encoded = try! TextCodec().encode(content: body)
		// swiftlint:enable force_try
        return DecodedMessage(topic: topic, encodedContent: encoded, senderAddress: senderAddress, sent: sent)
	}
}
