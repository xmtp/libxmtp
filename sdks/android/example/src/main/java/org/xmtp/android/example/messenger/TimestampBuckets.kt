package org.xmtp.android.example.messenger

data class BucketPage<T>(
    val rows: List<T>,
    val nextBeforeNs: Long?,
    val complete: Boolean,
    val notice: String? = null,
)

/** Counts use the same raw selection as reads
. A short lifted list proves nothing. */
class TimestampBuckets<T>(
    private val sentAt: (T) -> Long,
) {
    suspend fun load(
        beforeNs: Long?,
        read: suspend (
            Long?,
            Int,
        ) -> List<T>,
        count: suspend (
            Long?,
            Long?,
        ) -> ULong,
    ): BucketPage<T> {
        for (limit in listOf(
            51,
            101,
            201,
            501,
        )) {
            val rows =
                read(
                    beforeNs,
                    limit,
                ).sortedByDescending(sentAt)
            val rawCount =
                count(
                    beforeNs,
                    null,
                )
            if (rawCount == 0uL) {
                return BucketPage(
                    emptyList(),
                    null,
                    true,
                )
            }
            if (rows
                    .isEmpty()
            ) {
                return BucketPage(
                    emptyList(),
                    beforeNs,
                    false,
                    "Some stored history cannot be read.",
                )
            }
            // One row is a sentinel. Never advance past an unretained member of a tie.
            val retainedLimit = if (limit == 51) 50 else 500
            if (rawCount <=
                limit
                    .toULong() &&
                rows.size <= retainedLimit
            ) {
                return BucketPage(
                    rows,
                    rows
                        .lastOrNull()
                        ?.let(sentAt),
                    true,
                )
            }
            val timestamps =
                rows
                    .map(sentAt)
                    .distinct()
            for (timestamp in timestamps
                .asReversed()) {
                val lower =
                    if (timestamp ==
                        Long.MIN_VALUE
                    ) {
                        null
                    } else {
                        timestamp - 1
                    }
                val coveredCount =
                    count(
                        beforeNs,
                        lower,
                    )
                val prefix =
                    rows.filter {
                        sentAt(it) >= timestamp
                    }
                if (coveredCount <=
                    limit
                        .toULong() && prefix.size <= retainedLimit
                ) {
                    return BucketPage(
                        prefix,
                        timestamp,
                        coveredCount == rawCount,
                    )
                }
            }
        }
        return BucketPage(
            emptyList(),
            beforeNs,
            false,
            "More history at this time cannot be loaded with this SDK.",
        )
    }
}

/** Retain three transcripts and trim the end farthest from the viewport. */
class TranscriptCache<T>(
    private val id: (T) -> String,
    private val sentAt: (T) -> Long,
) {
    private val entries =
        LinkedHashMap<
            String,
            List<T>,
        >(
            4,
            0.75f,
            true,
        )

    fun get(key: String): List<T>? = entries[key]

    fun put(
        key: String,
        rows: List<T>,
        anchorId: String? = null,
    ): List<T> {
        val sorted =
            rows
                .associateBy(id)
                .values
                .sortedWith(
                    compareByDescending(sentAt)
                        .thenBy(id),
                )
        val anchor =
            sorted
                .indexOfFirst {
                    id(it) == anchorId
                }.coerceAtLeast(0)
        val start =
            (anchor - 250)
                .coerceIn(
                    0,
                    (
                        sorted
                            .size - 500
                    ).coerceAtLeast(0),
                )
        val retained =
            sorted
                .drop(start)
                .take(500)
        entries[key] = retained
        while (entries.size > 3) {
            entries
                .remove(
                    entries.keys
                        .first(),
                )
        }
        return retained
    }

    fun clear() =
        entries
            .clear()

    /** Derive coverage from the retained window, not from rows removed by the cache. */
    fun retainPage(
        key: String,
        previous: List<T>,
        page: BucketPage<T>,
        anchorId: String?,
    ): BucketPage<T> {
        val merged = (previous + page.rows).associateBy(id).values.toList()
        val retained = put(key, merged, anchorId)
        val oldest = retained.minOfOrNull(sentAt) ?: return page.copy(rows = retained)
        val retainedIds = retained.map(id).toSet()
        val removed = merged.filter { id(it) !in retainedIds && sentAt(it) <= oldest }
        if (removed.any { sentAt(it) == oldest }) {
            return BucketPage(
                retained,
                if (oldest == Long.MAX_VALUE) null else oldest + 1,
                false,
                "More history at this time cannot be kept in this window. Jump to latest to change the position.",
            )
        }
        return if (removed.isNotEmpty()) {
            BucketPage(
                retained,
                oldest,
                false,
                page.notice,
            )
        } else {
            page.copy(rows = retained)
        }
    }
}
