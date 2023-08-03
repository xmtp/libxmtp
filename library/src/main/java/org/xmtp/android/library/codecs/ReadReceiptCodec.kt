package org.xmtp.android.library.codecs

import com.google.protobuf.ByteString
import org.xmtp.android.library.XMTPException

val ContentTypeReadReceipt = ContentTypeIdBuilder.builderFromAuthorityId(
    "xmtp.org",
    "readReceipt",
    versionMajor = 1,
    versionMinor = 0
)

data class ReadReceipt(
    // The timestamp the read receipt was sent, in ISO 8601 format
    val timestamp: String,
)

data class ReadReceiptCodec(override var contentType: ContentTypeId = ContentTypeReadReceipt) :
    ContentCodec<ReadReceipt> {

    override fun encode(content: ReadReceipt): EncodedContent {
        return EncodedContent.newBuilder().also {
            it.type = ContentTypeReadReceipt
            it.putParameters("timestamp", content.timestamp)
            it.content = ByteString.EMPTY
        }.build()
    }

    override fun decode(content: EncodedContent): ReadReceipt {
        val timestamp = content.parametersMap["timestamp"] ?: throw XMTPException("Invalid Content")

        return ReadReceipt(timestamp = timestamp)
    }
}
