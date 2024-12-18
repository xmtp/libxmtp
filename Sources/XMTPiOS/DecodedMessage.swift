import Foundation

/// Decrypted messages from a conversation.
public struct DecodedMessage: Sendable {
	public var topic: String

	public var id: String = ""

	public var encodedContent: EncodedContent

	/// The wallet address of the sender of the message
	public var senderInboxId: String

	/// When the message was sent
	public var sent: Date
	public var sentNs: Int64

	public var client: Client

	public var deliveryStatus: MessageDeliveryStatus = .published

	init(
		id: String,
		client: Client,
		topic: String,
		encodedContent: EncodedContent,
		senderInboxId: String,
		sent: Date,
		sentNs: Int64,
		deliveryStatus: MessageDeliveryStatus = .published
	) {
		self.id = id
		self.client = client
		self.topic = topic
		self.encodedContent = encodedContent
		self.senderInboxId = senderInboxId
		self.sent = sent
		self.sentNs = sentNs
		self.deliveryStatus = deliveryStatus
	}

	public func content<T>() throws -> T {
		return try encodedContent.decoded(with: client)
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
