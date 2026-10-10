package org.xmtp.android.example.messenger

import uniffi.xmtp_sdk.*

suspend fun Conversation.historyPage(
    options: ListMessagesOptions,
    before: MessageHistoryPosition? = null,
    after: MessageHistoryPosition? = null,
): MessageHistoryPage =
    when (this) {
        is Conversation.Group -> group.messageHistoryPage(options, before, after)
        is Conversation.Dm -> dm.messageHistoryPage(options, before, after)
    }

fun Message.historyPosition(): MessageHistoryPosition? = deliveryCursor?.let { MessageHistoryPosition(sentAt, it) }

data class HistoryQueryPage<T>(
    val rows: List<T>,
    val first: MessageHistoryPosition?,
    val last: MessageHistoryPosition?,
    val hasMore: Boolean,
    val skipped: UInt,
)

fun MessageHistoryPage.queryPage() = HistoryQueryPage(messages, firstPosition, lastPosition, hasMore, skippedCount)

data class HistoryWindow<T>(
    val rows: List<T>,
    val first: MessageHistoryPosition?,
    val last: MessageHistoryPosition?,
    val hasOlder: Boolean,
    val atNewest: Boolean,
    val notice: String? = null,
    val newerCount: Int = 0,
    val changed: Boolean = false,
)

/** Query boundaries are opaque SDK positions. Empty converted pages can still advance. */
class SDKHistoryPages<T>(
    private val readableRow: (T) -> Boolean = { true },
    private val read: suspend (
        MessageOrder,
        MessageHistoryPosition?,
        MessageHistoryPosition?,
    ) -> HistoryQueryPage<T>,
) {
    private var remaining = 4

    private suspend fun readable(
        direction: MessageOrder,
        before: MessageHistoryPosition?,
        after: MessageHistoryPosition?,
        budget: Int,
    ): HistoryQueryPage<T> {
        var upper = before
        var lower = after
        var first: MessageHistoryPosition? = null
        var skipped = 0u
        var result: HistoryQueryPage<T>
        var used = 0
        do {
            check(remaining > 0) { "History read budget exceeded" }
            remaining -= 1
            used += 1
            result = read(direction, upper, lower)
            first = first ?: result.first
            skipped += result.skipped
            if (result.hasMore) {
                val next = checkNotNull(result.last) { "SDK history continuation is missing" }
                if (direction == MessageOrder.DESCENDING) upper = next else lower = next
            }
        } while (result.rows.isEmpty() && result.hasMore && used < budget && remaining > 0)
        return result.copy(first = first, skipped = skipped)
    }

    private fun notice(skipped: UInt): String? =
        if (skipped > 0u) "Some stored history cannot be read. Load more to continue." else null

    suspend fun older(before: MessageHistoryPosition? = null): HistoryWindow<T> {
        val page = readable(MessageOrder.DESCENDING, before, null, remaining)
        return HistoryWindow(page.rows, page.first, page.last, page.hasMore, before == null, notice(page.skipped))
    }

    suspend fun around(anchor: MessageHistoryPosition): HistoryWindow<T> {
        val newer = readable(MessageOrder.ASCENDING, null, anchor, 2)
        val older = readable(MessageOrder.DESCENDING, newer.first, null, remaining)
        val window =
            HistoryWindow(
                newer.rows.asReversed() + older.rows,
                newer.last ?: older.first,
                older.last,
                older.hasMore,
                !newer.hasMore,
                notice(newer.skipped + older.skipped),
                newer.rows.size,
            )
        return if (window.rows.none(readableRow) && remaining > 0) {
            val newest = older()
            newest.copy(changed = true, notice = window.notice ?: newest.notice)
        } else {
            window
        }
    }
}

/** Keep SDK window order. IDs identify rows but never order them. */
class SDKTranscriptCache<T>(
    private val id: (T) -> String,
    private val position: (T) -> MessageHistoryPosition?,
) {
    private val entries = LinkedHashMap<String, HistoryWindow<T>>(4, 0.75f, true)

    fun get(key: String): HistoryWindow<T>? = entries[key]

    fun clear() = entries.clear()

    fun put(
        key: String,
        window: HistoryWindow<T>,
        anchorId: String?,
    ): HistoryWindow<T> {
        val rows = window.rows.distinctBy(id)
        val anchor = rows.indexOfFirst { id(it) == anchorId }.coerceAtLeast(0)
        val start = (anchor - 250).coerceIn(0, (rows.size - 500).coerceAtLeast(0))
        val retained = rows.drop(start).take(500)
        val trimmedOlder = start + retained.size < rows.size
        val kept =
            window.copy(
                rows = retained,
                first = if (start > 0) retained.firstOrNull()?.let(position) else window.first,
                last = if (trimmedOlder) retained.lastOrNull()?.let(position) else window.last,
                hasOlder = trimmedOlder || window.hasOlder,
                atNewest = start == 0 && window.atNewest,
            )
        entries[key] = kept
        while (entries.size > 3) entries.remove(entries.keys.first())
        return kept
    }

    fun append(
        key: String,
        page: HistoryWindow<T>,
        anchorId: String?,
    ): HistoryWindow<T> {
        val previous = entries[key] ?: return put(key, page, anchorId)
        return put(
            key,
            page.copy(
                rows = previous.rows + page.rows,
                first = previous.first,
                atNewest = previous.atNewest,
                notice = previous.notice ?: page.notice,
            ),
            anchorId,
        )
    }
}
