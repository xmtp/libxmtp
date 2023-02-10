package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import java.util.Date

typealias Envelope = org.xmtp.proto.message.api.v1.MessageApiOuterClass.Envelope

class EnvelopeBuilder {
    companion object {
        fun buildFromString(topic: String, timestamp: Date, message: ByteArray): Envelope {
            return Envelope.newBuilder().apply {
                contentTopic = topic
                timestampNs = (timestamp.time * 1_000_000)
                this.message = message.toByteString()
            }.build()
        }

        fun buildFromTopic(topic: Topic, timestamp: Date, message: ByteArray): Envelope {
            return Envelope.newBuilder().apply {
                contentTopic = topic.description
                timestampNs = (timestamp.time * 1_000_000)
                this.message = message.toByteString()
            }.build()
        }
    }
}
