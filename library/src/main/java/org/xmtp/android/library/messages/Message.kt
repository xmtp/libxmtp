package org.xmtp.android.library.messages

typealias Message = org.xmtp.proto.message.contents.MessageOuterClass.Message

enum class MessageVersion(val rawValue: String) {
    V1("v1"),
    V2("v2");

    companion object {
        operator fun invoke(rawValue: String) =
            MessageVersion.values().firstOrNull { it.rawValue == rawValue }
    }
}

class MessageBuilder {
    companion object {
        fun buildFromMessageV1(v1: MessageV1): Message {
            return Message.newBuilder().also {
                it.v1 = v1
            }.build()
        }

        fun buildFromMessageV2(v2: MessageV2): Message {
            return Message.newBuilder().also {
                it.v2 = v2
            }.build()
        }
    }
}
