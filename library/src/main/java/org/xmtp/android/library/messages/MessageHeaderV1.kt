package org.xmtp.android.library.messages

typealias MessageHeaderV1 = org.xmtp.proto.message.contents.MessageOuterClass.MessageHeaderV1

class MessageHeaderV1Builder {
    companion object {
        fun buildFromPublicBundles(
            sender: PublicKeyBundle,
            recipient: PublicKeyBundle,
            timestamp: Long
        ): MessageHeaderV1 {
            return MessageHeaderV1.newBuilder().also {
                it.sender = sender
                it.recipient = recipient
                it.timestamp = timestamp
            }.build()
        }
    }
}
