package org.xmtp.android.library.messages

import org.xmtp.android.library.codecs.EncodedContent
import java.util.Date

data class DecryptedMessage(
    var id: String,
    var encodedContent: EncodedContent,
    var senderAddress: String,
    var sentAt: Date,
    var topic: String = "",
    var deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.PUBLISHED
)
