package org.xmtp.android.library.libxmtp

import org.xmtp.android.library.Client
import org.xmtp.android.library.DecodedMessage
import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.toHex
import uniffi.xmtpv3.FfiMessage
import java.util.Date

data class Message(val client: Client, private val libXMTPMessage: FfiMessage) {
    val id: ByteArray
        get() = libXMTPMessage.id

    val senderAddress: String
        get() = libXMTPMessage.addrFrom

    val sentAt: Date
        get() = Date(libXMTPMessage.sentAtNs / 1_000_000)

    fun decode(): DecodedMessage {
        try {
            return DecodedMessage(
                id = id.toHex(),
                client = client,
                topic = id.toHex(),
                encodedContent = EncodedContent.parseFrom(libXMTPMessage.content),
                senderAddress = senderAddress,
                sent = sentAt
            )
        } catch (e: Exception) {
            throw XMTPException("Error decoding message", e)
        }
    }
}
