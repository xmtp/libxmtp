package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.codecs.compress
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.Message
import org.xmtp.android.library.messages.MessageBuilder
import org.xmtp.android.library.messages.MessageV2
import org.xmtp.android.library.messages.MessageV2Builder
import org.xmtp.android.library.messages.SealedInvitationHeaderV1
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.contents.Invitation
import java.util.Date

data class ConversationV2(
    var topic: String,
    var keyMaterial: ByteArray,
    var context: Invitation.InvitationV1.Context,
    var peerAddress: String,
    var client: Client,
    private var header: SealedInvitationHeaderV1,
) {
    companion object {

        fun create(
            client: Client,
            invitation: Invitation.InvitationV1,
            header: SealedInvitationHeaderV1,
        ): ConversationV2 {
            val myKeys = client.keys?.getPublicKeyBundle()
            val peer =
                if (myKeys?.walletAddress == (header.sender.walletAddress)) header.recipient else header.sender
            val peerAddress = peer.walletAddress
            val keyMaterial = invitation.aes256GcmHkdfSha256.keyMaterial.toByteArray()
            return ConversationV2(
                topic = invitation.topic,
                keyMaterial = keyMaterial,
                context = invitation.context,
                peerAddress = peerAddress,
                client = client,
                header = header
            )
        }
    }

    val createdAt: Date = Date((header.createdNs / 1_000_000) / 1000)

    fun messages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
    ): List<DecodedMessage> {
        val envelopes =
            runBlocking { client.apiClient.queryStrings(topics = listOf(topic)).envelopesList }
        return envelopes.flatMap { envelope ->
            val message = Message.parseFrom(envelope.message)
            listOf(decode(message.v2))
        }
    }

    private fun decode(message: MessageV2): DecodedMessage =
        MessageV2Builder.buildDecode(message, keyMaterial = keyMaterial)

    fun <T> send(content: T, options: SendOptions? = null) {
        val codec = Client.codecRegistry.find(options?.contentType)

        fun <Codec : ContentCodec<T>> encode(codec: Codec, content: Any?): EncodedContent {
            val contentType = content as? T
            if (contentType != null) {
                return codec.encode(contentType)
            } else {
                throw XMTPException("Codec type is not registered")
            }
        }

        var encoded = encode(codec = codec as ContentCodec<T>, content = content)
        encoded = encoded.toBuilder().also {
            it.fallback = options?.contentFallback ?: ""
        }.build()
        send(encodedContent = encoded, sentAt = Date())
    }

    fun send(text: String, options: SendOptions? = null, sentAt: Date) {
        val encoder = TextCodec()
        val encodedContent = encoder.encode(content = text)
        send(encodedContent = encodedContent, options = options, sentAt = sentAt)
    }

    private fun send(encodedContent: EncodedContent, options: SendOptions? = null, sentAt: Date) {
        if (client.getUserContact(peerAddress = peerAddress) == null) {
            throw XMTPException("Contact not found.")
        }
        var content = encodedContent

        if (options?.compression != null) {
            content = content.compress(options.compression!!)
        }

        val message = MessageV2Builder.buildEncode(
            client = client,
            encodedContent = content,
            topic = topic,
            keyMaterial = keyMaterial
        )
        client.publish(
            envelopes = listOf(
                EnvelopeBuilder.buildFromString(
                    topic = topic,
                    timestamp = sentAt,
                    message = MessageBuilder.buildFromMessageV2(message).toByteArray()
                )
            )
        )
    }

    fun send(text: String, options: SendOptions? = null) {
        send(text = text, options = options, sentAt = Date())
    }
}
