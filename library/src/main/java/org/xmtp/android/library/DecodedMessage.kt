package org.xmtp.android.library

import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.codecs.decoded
import org.xmtp.proto.message.contents.Content
import java.util.Date

data class DecodedMessage(
    var id: String = "",
    val client: Client,
    var topic: String,
    var encodedContent: Content.EncodedContent,
    var senderAddress: String,
    var sent: Date,
) {
    companion object {
        fun preview(client: Client, topic: String, body: String, senderAddress: String, sent: Date): DecodedMessage {
            val encoded = TextCodec().encode(content = body)
            return DecodedMessage(
                client = client,
                topic = topic,
                encodedContent = encoded,
                senderAddress = senderAddress,
                sent = sent
            )
        }
    }

    fun <T> content(): T? =
        encodedContent.decoded()

    val fallbackContent: String
        get() = encodedContent.fallback

    val body: String
        get() {
            return content() as String? ?: fallbackContent
        }
}
