package org.xmtp.android.example.messenger
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class TimestampBucketTest {
    private data class Stored(
        val id: Int,
        val time: Long,
        val readable: Boolean = true,
    )

    private suspend fun load(
        raw: List<Stored>,
        before: Long? = null,
    ): BucketPage<Stored> =
        TimestampBuckets<Stored>({
            it.time
        })
            .load(
                before,
                read = {
                    upper,
                    limit,
                    ->
                    raw
                        .filter {
                            upper == null || it.time < upper
                        }.sortedByDescending {
                            it.time
                        }.take(
                            limit,
                        ).filter {
                            it.readable
                        }
                },
                count = {
                    upper,
                    lower,
                    ->
                    raw
                        .count {
                            (
                                upper == null || it.time < upper
                            ) && (
                                lower == null || it.time > lower
                            )
                        }.toULong()
                },
            )

    @Test fun conversionShortPageDoesNotSkipTheRestOfAnEightyRowTie() =
        runBlocking {
            val raw =
                (0 until 80).map {
                    Stored(
                        it,
                        10,
                        it != 5,
                    )
                }
            val page = load(raw)
            assertEquals(
                79,
                page.rows.size,
            )
            assertEquals(
                raw
                    .filter {
                        it.readable
                    }.map {
                        it.id
                    }.toSet(),
                page.rows
                    .map {
                        it.id
                    }.toSet(),
            )
            assertTrue(
                page.complete,
            )
        }

    @Test fun fiveHundredAndOneReadableRowsStopWithoutAdvancingPastAnUnretainedTie() =
        runBlocking {
            val page =
                load(
                    (0 until 501).map {
                        Stored(
                            it,
                            10,
                        )
                    },
                    20,
                )
            assertEquals(
                20L,
                page.nextBeforeNs,
            )
            assertTrue(
                page.rows
                    .isEmpty(),
            )
            assertEquals(
                "More history at this time cannot be loaded with this SDK.",
                page.notice,
            )
            assertFalse(
                page.complete,
            )
        }

    @Test fun completeNewerPrefixCanAdvanceWithoutCrossingTheLargeBucket() =
        runBlocking {
            val raw =
                listOf(
                    Stored(
                        -1,
                        20,
                    ),
                ) +
                    (0 until 501).map {
                        Stored(
                            it,
                            10,
                        )
                    }
            val page = load(raw)
            assertEquals(
                listOf(-1),
                page.rows.map {
                    it.id
                },
            )
            assertEquals(
                20L,
                page.nextBeforeNs,
            )
            assertFalse(
                page.complete,
            )
        }

    @Test fun unreadableRawHistoryIsNotAnEmptyConversation() =
        runBlocking {
            val page =
                load(
                    listOf(
                        Stored(
                            1,
                            Long.MIN_VALUE,
                            false,
                        ),
                    ),
                )
            assertFalse(
                page.complete,
            )
            assertNotNull(
                page.notice,
            )
        }

    @Test fun cacheBoundsAndDeduplicatesAroundTheViewport() {
        val cache =
            TranscriptCache<Stored>(
                {
                    it.id
                        .toString()
                },
                {
                    it.time
                },
            )
        val rows =
            (0 until 1000).map {
                Stored(
                    it,
                    it
                        .toLong(),
                )
            }
        val kept =
            cache
                .put(
                    "a",
                    rows + rows,
                    "100",
                )
        assertEquals(
            500,
            kept.size,
        )
        assertEquals(
            500,
            kept
                .map {
                    it.id
                }.toSet()
                .size,
        )
        assertTrue(
            kept.any {
                it.id == 100
            },
        )
        cache
            .put(
                "b",
                rows,
            )
        cache
            .put(
                "c",
                rows,
            )
        cache
            .put(
                "d",
                rows,
            )
        assertNull(
            cache
                .get("a"),
        )
    }
}
