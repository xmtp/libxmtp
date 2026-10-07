import Foundation
import XmtpSdk

final class NonSendableValue {
	var text = "value"
}

struct NonSendableCodec: ContentCodec {
	let type = ContentTypeId(authorityId: "example.org", typeId: "non-sendable", versionMajor: 1, versionMinor: 0)

	func encode(_: NonSendableValue) throws -> EncodedContent {
		EncodedContent(type: type, content: Data())
	}

	func decode(_: EncodedContent) throws -> NonSendableValue {
		NonSendableValue()
	}
}

final class MutableCodec: ContentCodec {
	var count = 0
	let type = ContentTypeId(authorityId: "example.org", typeId: "mutable", versionMajor: 1, versionMinor: 0)

	func encode(_: Int) throws -> EncodedContent {
		count += 1
		return EncodedContent(type: type, content: Data())
	}

	func decode(_: EncodedContent) throws -> Int {
		count
	}
}

func consumeNegative(_ conversation: Conversation, _ content: MessageContent) {
	let _: ConversationId = 42
	_ = MessageId.fromString("bad")
	let _: EncodedContent = content
	let _: Group = conversation
	let _: StandardContent = .deleteMessage(messageId: 42)
}
