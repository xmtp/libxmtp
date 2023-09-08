package org.xmtp.android.library.codecs

import com.google.protobuf.ByteString
import org.xmtp.android.library.XMTPException

val ContentTypeAttachment = ContentTypeIdBuilder.builderFromAuthorityId(
    "xmtp.org",
    "attachment",
    versionMajor = 1,
    versionMinor = 0
)

data class Attachment(val filename: String, val mimeType: String, val data: ByteString)

data class AttachmentCodec(override var contentType: ContentTypeId = ContentTypeAttachment) : ContentCodec<Attachment> {
    override fun encode(content: Attachment): EncodedContent {
        return EncodedContent.newBuilder().also {
            it.type = ContentTypeAttachment
            it.putAllParameters(mapOf("filename" to content.filename, "mimeType" to content.mimeType))
            it.content = content.data
        }.build()
    }

    override fun decode(content: EncodedContent): Attachment {
        val filename = content.parametersMap["filename"] ?: throw XMTPException("missing filename")
        val mimeType = content.parametersMap["mimeType"] ?: throw XMTPException("missing mimeType")
        val encodedContent = content.content ?: throw XMTPException("missing content")

        return Attachment(
            filename = filename,
            mimeType = mimeType,
            data = encodedContent,
        )
    }

    override fun fallback(content: Attachment): String? {
        return "Can’t display \"${content.filename}”. This app doesn’t support attachments."
    }
}
