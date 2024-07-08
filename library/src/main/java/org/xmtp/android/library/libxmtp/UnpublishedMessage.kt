package uniffi.xmtpv3.org.xmtp.android.library.libxmtp

import org.xmtp.android.library.toHex
import uniffi.xmtpv3.FfiUnpublishedMessage

class UnpublishedMessage(private val libXMTPUnpublishedMessage: FfiUnpublishedMessage) {
    val messageId: String
        get() = libXMTPUnpublishedMessage.id().toHex()

    suspend fun publish() {
        libXMTPUnpublishedMessage.publish()
    }
}
