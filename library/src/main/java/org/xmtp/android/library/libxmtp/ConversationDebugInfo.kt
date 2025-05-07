package uniffi.xmtpv3.org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiConversationDebugInfo

class ConversationDebugInfo(private val ffiConversationDebugInfo: FfiConversationDebugInfo) {
    val epoch: Long
        get() = ffiConversationDebugInfo.epoch.toLong()
    val maybeForked: Boolean
        get() = ffiConversationDebugInfo.maybeForked

    val forkDetails: String
        get() = ffiConversationDebugInfo.forkDetails
}
