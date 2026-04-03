package org.xmtp.kotlin

import org.xmtp.kotlin.codecs.ContentCodec
import org.xmtp.kotlin.codecs.ContentTypeId
import org.xmtp.kotlin.codecs.TextCodec
import org.xmtp.kotlin.codecs.id

data class CodecRegistry(
    val codecs: MutableMap<String, ContentCodec<*>> = mutableMapOf(),
) {
    fun register(codec: ContentCodec<*>) {
        codecs[codec.contentType.id] = codec
    }

    fun find(contentType: ContentTypeId?): ContentCodec<*> {
        contentType?.let {
            val codec = codecs[it.id]
            if (codec != null) {
                return codec
            }
        }
        return TextCodec()
    }

    fun findFromId(contentTypeString: String): ContentCodec<*> {
        for ((_, codec) in codecs) {
            if (codec.contentType.id == contentTypeString) {
                return codec
            }
        }
        return TextCodec()
    }
}
