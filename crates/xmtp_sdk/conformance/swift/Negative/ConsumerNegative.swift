import Foundation
import XmtpSdk

final class NonSendableValue {
    var text = "value"
}

struct NonSendableCodec: SDKContentCodec {
    let type = ContentTypeId(authorityId: "example.org", typeId: "non-sendable", versionMajor: 1, versionMinor: 0)

    func encode(_: Any) throws -> EncodedContent {
        EncodedContent(type: type, content: Data())
    }

    func decode(_: EncodedContent) throws -> Any {
        NonSendableValue()
    }
}

final class MutableCodec: SDKContentCodec {
    var count = 0
    let type = ContentTypeId(authorityId: "example.org", typeId: "mutable", versionMajor: 1, versionMinor: 0)

    func encode(_: any Sendable) throws -> EncodedContent {
        count += 1
        return EncodedContent(type: type, content: Data())
    }

    func decode(_: EncodedContent) throws -> any Sendable {
        count
    }
}

func consumeNegative(_ conversation: Conversation, _ content: MessageContent) {
    let _: ConversationId = 42
    let _ = MessageId.fromString("bad")
    let _: EncodedContent = content
    let _: Group = conversation
    let _: StandardContent = .deleteMessage(messageId: 42)
}
