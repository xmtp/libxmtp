package org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiConversationDebugInfo

class ConversationDebugInfo(
    private val ffiConversationDebugInfo: FfiConversationDebugInfo,
) {
    enum class CommitLogForkStatus {
        FORKED,
        NOT_FORKED,
        UNKNOWN,
    }

    val epoch: Long
        get() = ffiConversationDebugInfo.epoch.toLong()
    val maybeForked: Boolean
        get() = ffiConversationDebugInfo.maybeForked
    val forkDetails: String
        get() = ffiConversationDebugInfo.forkDetails
    val localCommitLog: String
        get() = ffiConversationDebugInfo.localCommitLog
    val remoteCommitLog: String
        get() = ffiConversationDebugInfo.remoteCommitLog
    val commitLogForkStatus: CommitLogForkStatus
        get() =
            when (ffiConversationDebugInfo.isCommitLogForked) {
                true -> CommitLogForkStatus.FORKED
                false -> CommitLogForkStatus.NOT_FORKED
                null -> CommitLogForkStatus.UNKNOWN
            }
}
