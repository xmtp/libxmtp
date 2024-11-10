package org.xmtp.android.library

import org.xmtp.android.library.codecs.decoded
import org.xmtp.android.library.libxmtp.Message.MessageDeliveryStatus
import org.xmtp.proto.message.contents.Content
import java.util.Date

data class DecodedMessage(
    var id: String = "",
    val client: Client,
    var topic: String,
    var encodedContent: Content.EncodedContent,
    var senderAddress: String,
    var sent: Date,
    var sentNs: Long,
    var deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.PUBLISHED
) {
    fun <T> content(): T? =
        encodedContent.decoded()

    val fallbackContent: String
        get() = encodedContent.fallback

    val body: String
        get() {
            return content() as String? ?: fallbackContent
        }
}
