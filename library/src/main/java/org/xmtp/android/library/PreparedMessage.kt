package org.xmtp.android.library

import org.web3j.crypto.Hash
import org.xmtp.android.library.messages.Envelope

data class PreparedMessage(
    var messageEnvelope: Envelope,
    var conversation: Conversation,
    var onSend: () -> Unit,
) {

    fun decodedMessage(): DecodedMessage =
        conversation.decode(messageEnvelope)

    fun send() {
        onSend()
    }

    val messageId: String
        get() = Hash.sha256(messageEnvelope.message.toByteArray()).toHex()
}
