package org.xmtp.android.library

import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.ContentTypeId
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.codecs.id

data class CodecRegistry(val codecs: MutableMap<String, ContentCodec<*>> = mutableMapOf()) {

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
