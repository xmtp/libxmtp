package org.xmtp.android.example.messenger
import uniffi.xmtp_sdk.*

internal suspend fun Conversation.recoveryPage(
    options: ListMessagesOptions,
    before: MessageRecoveryPosition? = null,
    after: MessageRecoveryPosition? = null,
): MessageRecoveryPage =
    when (this) {
        is Conversation.Group -> group.messageRecoveryPage(options, before, after)
        is Conversation.Dm -> dm.messageRecoveryPage(options, before, after)
    }

internal data class PendingMessagePage(
    val rows: List<Message>,
    val first: MessageRecoveryPosition?,
    val last: MessageRecoveryPosition?,
    val hasOlder: Boolean,
    val notice: String?,
)

/** Keep SDK order and raw bounds. Each recovery operation reads at most four pages. */
internal suspend fun pendingMessagePage(
    before: MessageRecoveryPosition?,
    read: suspend (ListMessagesOptions, MessageRecoveryPosition?, MessageRecoveryPosition?) -> MessageRecoveryPage,
): PendingMessagePage {
    val options = publishedSelection().copy(deliveryStatus = null, limit = 50u)
    var upper = before
    var first: MessageRecoveryPosition? = null
    var skipped = 0u
    var page: MessageRecoveryPage
    var reads = 0
    do {
        page = read(options, upper, null)
        reads += 1
        first = first ?: page.firstPosition
        skipped += page.skippedCount
        if (page.hasMore) {
            upper = checkNotNull(page.lastPosition) { "SDK recovery continuation is missing" }
        }
    } while (page.messages.isEmpty() && page.hasMore && reads < 4)
    return PendingMessagePage(
        rows = page.messages,
        first = first,
        last = page.lastPosition,
        hasOlder = page.hasMore,
        notice =
            if (skipped > 0u) {
                if (page.hasMore) {
                    "Some stored pending messages cannot be read. Load more to continue."
                } else {
                    "Some stored pending messages cannot be read. Refresh to try again."
                }
            } else {
                null
            },
    )
}
