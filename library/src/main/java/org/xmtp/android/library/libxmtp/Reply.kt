package org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiEnrichedReply

data class Reply(
    val inReplyTo: DecodedMessageV2?,
    val content: Any?,
    val referenceId: String
) {
    companion object {
        fun create(ffiEnrichedReply: FfiEnrichedReply): Reply {
            val inReplyTo = ffiEnrichedReply.inReplyTo?.let { DecodedMessageV2.create(it) }

            // Use the centralized decoding logic from DecodedMessageV2
            val content = ffiEnrichedReply.content?.let { body ->
                DecodedMessageV2.decodeBodyContent(body)
            }

            return Reply(
                inReplyTo = inReplyTo,
                content = content,
                referenceId = ffiEnrichedReply.referenceId
            )
        }
    }
}
