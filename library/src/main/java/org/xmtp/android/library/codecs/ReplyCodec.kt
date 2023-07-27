package org.xmtp.android.library.codecs

import com.google.gson.GsonBuilder
import com.google.protobuf.kotlin.toByteStringUtf8

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

    override fun encode(content: Reply): EncodedContent {
        val gson = GsonBuilder().create()
        return EncodedContent.newBuilder().also {
            it.type = ContentTypeReply
            it.content = gson.toJson(content).toByteStringUtf8()
        }.build()
    }

    override fun decode(content: EncodedContent): Reply {
        val gson = GsonBuilder().create()
        return gson.fromJson(content.content.toStringUtf8(), Reply::class.java)
    }
}
