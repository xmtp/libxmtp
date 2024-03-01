package org.xmtp.android.library.codecs

import com.google.protobuf.ByteString

val ContentTypeReadReceipt = ContentTypeIdBuilder.builderFromAuthorityId(
    "xmtp.org",
    "readReceipt",
    versionMajor = 1,
    versionMinor = 0,
)

object ReadReceipt

data class ReadReceiptCodec(override var contentType: ContentTypeId = ContentTypeReadReceipt) :
    ContentCodec<ReadReceipt> {

    override fun encode(content: ReadReceipt): EncodedContent {
        return EncodedContent.newBuilder().also {
            it.type = ContentTypeReadReceipt
            it.content = ByteString.EMPTY
        }.build()
    }

    override fun decode(content: EncodedContent): ReadReceipt {
        return ReadReceipt
    }

    override fun fallback(content: ReadReceipt): String? {
        return null
    }

    override fun shouldPush(content: ReadReceipt): Boolean = false
}
