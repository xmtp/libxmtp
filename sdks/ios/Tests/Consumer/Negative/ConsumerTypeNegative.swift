import Foundation
import XmtpSdk

func consumeNegative(_ conversation: Conversation, _ content: MessageContent) {
	let _: ConversationId = 42
	_ = MessageId.fromString("bad")
	let _: EncodedContent = content
	let _: Group = conversation
	let _: StandardContent = .deleteMessage(messageId: 42)
}
