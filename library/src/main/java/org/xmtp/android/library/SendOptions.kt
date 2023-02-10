package org.xmtp.android.library

import org.xmtp.proto.message.contents.Content

data class SendOptions(
    var compression: EncodedContentCompression? = null,
    var contentType: Content.ContentTypeId? = null,
    var contentFallback: String? = null
)
