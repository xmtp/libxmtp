package org.xmtp.android.library.codecs

import com.google.protobuf.kotlin.toByteStringUtf8
import org.xmtp.android.library.XMTPException

val ContentTypeText = ContentTypeIdBuilder.builderFromAuthorityId(
    "xmtp.org",
    "text",
    versionMajor = 1,
    versionMinor = 0
)

data class TextCodec(override var contentType: ContentTypeId = ContentTypeText) :
    ContentCodec<String> {
    override fun encode(content: String): EncodedContent {
        return EncodedContent.newBuilder().also {
            it.type = ContentTypeText
            it.putAllParameters(mapOf("encoding" to "UTF-8"))
            it.content = content.toByteStringUtf8()
        }.build()
    }

    override fun decode(content: EncodedContent): String {
        val encoding = content.parameters["encoding"]
        if (encoding != null && encoding != "UTF-8") {
            throw XMTPException("Invalid encoding")
        }
        val contentString = content.content.toStringUtf8()
        if (contentString != null) {
            return contentString
        } else {
            throw XMTPException("Unknown decoding")
        }
    }
}
