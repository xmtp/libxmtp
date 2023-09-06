package org.xmtp.android.library

import org.web3j.crypto.Hash
import org.xmtp.android.library.messages.Envelope
import org.xmtp.proto.message.api.v1.MessageApiOuterClass.PublishRequest

// This houses a fully prepared message that can be published
// as soon as the API client has connectivity.
//
// To support persistence layers that queue pending messages (e.g. while offline)
// this struct supports serializing to/from bytes that can be written to disk or elsewhere.
// See toSerializedData() and fromSerializedData()
data class PreparedMessage(
    // The first envelope should send the message to the conversation itself.
    // Any more are for required intros/invites etc.
    // A client can just publish these when it has connectivity.
    val envelopes: List<Envelope>
) {
    companion object {
        fun fromSerializedData(data: ByteArray): PreparedMessage {
            val req = PublishRequest.parseFrom(data)
            return PreparedMessage(req.envelopesList)
        }
    }

    fun toSerializedData(): ByteArray {
        val req = PublishRequest.newBuilder()
            .addAllEnvelopes(envelopes)
            .build()
        return req.toByteArray()
    }

    val messageId: String
        get() = Hash.sha256(envelopes.first().message.toByteArray()).toHex()

    val conversationTopic: String
        get() = envelopes.first().contentTopic
}
