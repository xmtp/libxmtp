package org.xmtp.android.library

import org.xmtp.proto.message.contents.Content
import uniffi.xmtpv3.FfiSendMessageOpts

data class SendOptions(
    var compression: EncodedContentCompression? = null,
    var contentType: Content.ContentTypeId? = null,
    @Deprecated("This option is no longer supported and does nothing")
    var ephemeral: Boolean = false,
    // Optional idempotency key. Re-sending identical content with the same key
    // produces the same message id and is deduplicated. Defaults to a timestamp.
    var idempotencyKey: String? = null,
)

data class MessageVisibilityOptions(
    val shouldPush: Boolean,
    // Optional idempotency key. Re-sending identical content with the same key
    // produces the same message id and is deduplicated. Defaults to a timestamp.
    val idempotencyKey: String? = null,
) {
    fun toFfi(): FfiSendMessageOpts =
        FfiSendMessageOpts(shouldPush = shouldPush, idempotencyKey = idempotencyKey)
}
