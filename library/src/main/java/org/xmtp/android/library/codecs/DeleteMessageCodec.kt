package org.xmtp.android.library.codecs

/**
 * Represents a request to delete a message.
 *
 * This content type is used to request deletion of a specific message in a conversation.
 * The message will be deleted for all participants.
 *
 * @property messageId The ID of the message to delete
 */
data class DeleteMessageRequest(
    val messageId: String,
)

val ContentTypeDeleteMessageRequest =
    ContentTypeIdBuilder.builderFromAuthorityId(
        "xmtp.org",
        "deleteMessage",
        versionMajor = 1,
        versionMinor = 0,
    )

data class DeleteMessageCodec(
    override var contentType: ContentTypeId = ContentTypeDeleteMessageRequest,
) : ContentCodec<DeleteMessageRequest> {
    override fun encode(content: DeleteMessageRequest): EncodedContent {
        val ffi =
            uniffi.xmtpv3.FfiDeleteMessage(
                messageId = content.messageId,
            )

        return EncodedContent.parseFrom(
            uniffi.xmtpv3.encodeDeleteMessage(ffi),
        )
    }

    override fun decode(content: EncodedContent): DeleteMessageRequest {
        val decoded = uniffi.xmtpv3.decodeDeleteMessage(content.toByteArray())

        return DeleteMessageRequest(
            messageId = decoded.messageId,
        )
    }

    override fun fallback(content: DeleteMessageRequest): String? = null

    override fun shouldPush(content: DeleteMessageRequest): Boolean = false
}
