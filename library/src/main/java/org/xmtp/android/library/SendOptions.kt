package org.xmtp.android.library

import org.xmtp.proto.message.contents.Content
import uniffi.xmtpv3.FfiSendMessageOpts

data class SendOptions(
    var compression: EncodedContentCompression? = null,
    var contentType: Content.ContentTypeId? = null,
    @Deprecated("This option is no longer supported and does nothing")
    var ephemeral: Boolean = false,
)

data class MessageVisibilityOptions(
    val shouldPush: Boolean,
) {
    fun toFfi(): FfiSendMessageOpts = FfiSendMessageOpts(shouldPush = shouldPush)
}
