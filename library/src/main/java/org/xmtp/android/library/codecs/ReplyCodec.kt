package org.xmtp.android.library.codecs

import org.xmtp.android.library.Client
import org.xmtp.android.library.XMTPException

val ContentTypeReply = ContentTypeIdBuilder.builderFromAuthorityId(
    "xmtp.org",
    "reply",
    versionMajor = 1,
    versionMinor = 0
)

data class Reply(
    val reference: String,
    val content: Any,
    val contentType: ContentTypeId,
)

data class ReplyCodec(override var contentType: ContentTypeId = ContentTypeReply) :
    ContentCodec<Reply> {

    override fun encode(reply: Reply): EncodedContent {
        val replyCodec = Client.codecRegistry.find(reply.contentType)

        return EncodedContent.newBuilder().also {
            it.type = ContentTypeReply
            // TODO: cut when we're certain no one is looking for "contentType" here.
            it.putParameters("contentType", reply.contentType.description)
            it.putParameters("reference", reply.reference)
            it.content = encodeReply(replyCodec, reply.content).toByteString()
        }.build()
    }

    override fun decode(content: EncodedContent): Reply {
        val reference =
            content.getParametersOrThrow("reference") ?: throw XMTPException("Invalid Content")
        val replyEncodedContent = EncodedContent.parseFrom(content.content)
        val replyCodec = Client.codecRegistry.find(replyEncodedContent.type)
        val replyContent = replyCodec.decode(content = replyEncodedContent)
            ?: throw XMTPException("Invalid Content")
        return Reply(
            reference = reference,
            content = replyContent,
            contentType = replyCodec.contentType
        )
    }

    override fun fallback(content: Reply): String? {
        return "Replied with “${content.content}” to an earlier message"
    }

    private fun <Codec : ContentCodec<T>, T> encodeReply(
        codec: Codec,
        content: Any,
    ): EncodedContent {
        val reply = content as? T
        if (reply != null) {
            return codec.encode(content = reply)
        } else {
            throw XMTPException("Invalid Content")
        }
    }
}
