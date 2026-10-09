package org.xmtp.android.example.messenger

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.MessageHistoryPosition
import uniffi.xmtp_sdk.MessageOrder
import uniffi.xmtp_sdk.Timestamp

class SDKHistoryPageTest {
    private data class Stored(
        val id: String,
        val position: MessageHistoryPosition,
    )

    private fun rows(size: Int) =
        (0 until size).map { index ->
            Stored(
                (size - index).toString().padStart(4, '0'),
                MessageHistoryPosition(Timestamp(10), "opaque-${size - index}"),
            )
        }

    @Test fun equalTimeSdkWindowsKeepOrderAndContinueBeyondFiveHundredRows() =
        runBlocking {
            for (size in listOf(80, 501)) {
                val raw = rows(size)
                val cache = SDKTranscriptCache<Stored>({ it.id }, { it.position })
                var before: MessageHistoryPosition? = null
                val seen = mutableListOf<Stored>()
                do {
                    val page =
                        SDKHistoryPages<Stored> { direction, upper, lower ->
                            assertEquals(MessageOrder.DESCENDING, direction)
                            assertNull(lower)
                            val start =
                                upper?.let { position ->
                                    raw.indexOfFirst { it.position.deliveryCursor == position.deliveryCursor } + 1
                                } ?: 0
                            val selected = raw.drop(start).take(50)
                            HistoryQueryPage(
                                selected,
                                selected.firstOrNull()?.position,
                                selected.lastOrNull()?.position,
                                start + selected.size < raw.size,
                                0u,
                            )
                        }.older(before)
                    assertTrue(
                        "An SDK continuation must not repeat a retained ID",
                        page.rows.none { row -> seen.any { it.id == row.id } },
                    )
                    seen.addAll(page.rows)
                    before = page.last
                    val kept =
                        if (cache.get("chat") == null) {
                            cache.put("chat", page, page.rows.lastOrNull()?.id)
                        } else {
                            cache.append("chat", page, page.rows.lastOrNull()?.id)
                        }
                    assertTrue(kept.rows.size <= 500)
                    assertEquals(raw.filter { candidate -> kept.rows.any { it.id == candidate.id } }, kept.rows)
                } while (page.hasOlder)
                assertEquals(raw, seen)
                assertEquals(size, seen.map { it.id }.toSet().size)
                assertFalse(cache.get("chat")!!.hasOlder)
            }
        }

    @Test fun emptyConvertedPrefixUsesRawContinuationAndFourReadBudget() =
        runBlocking {
            val raw = rows(250)
            var calls = 0
            val read: suspend (
                MessageOrder,
                MessageHistoryPosition?,
                MessageHistoryPosition?,
            ) -> HistoryQueryPage<Stored> = { _, before, _ ->
                calls += 1
                val start =
                    before?.let { position ->
                        raw.indexOfFirst { it.position.deliveryCursor == position.deliveryCursor } + 1
                    } ?: 0
                val selected = raw.drop(start).take(50)
                val converted = selected.filter { raw.indexOf(it) >= 200 }
                HistoryQueryPage(
                    converted,
                    selected.firstOrNull()?.position,
                    selected.lastOrNull()?.position,
                    start + selected.size < raw.size,
                    (selected.size - converted.size).toUInt(),
                )
            }
            val prefix = SDKHistoryPages(read = read).older()
            assertEquals(4, calls)
            assertTrue(prefix.rows.isEmpty())
            assertTrue(prefix.hasOlder)
            assertNotNull(prefix.notice)
            assertEquals(raw[199].position.deliveryCursor, prefix.last!!.deliveryCursor)
            val readable = SDKHistoryPages(read = read).older(prefix.last)
            assertEquals(raw.drop(200), readable.rows)
            assertFalse(readable.hasOlder)
            assertEquals(5, calls)
        }

    @Test fun anchoredQueriesUseSdkNeighborsWithoutLookingUpTheDeletedAnchor() =
        runBlocking {
            val raw = rows(150)
            val saved = raw[80].position
            val retained = raw.filterIndexed { index, _ -> index != 80 }
            val requests = mutableListOf<Pair<MessageHistoryPosition?, MessageHistoryPosition?>>()
            val window =
                SDKHistoryPages<Stored> { direction, before, after ->
                    requests.add(before to after)
                    val selected =
                        if (direction == MessageOrder.ASCENDING) {
                            assertEquals(saved.deliveryCursor, after!!.deliveryCursor)
                            retained.filter { raw.indexOf(it) < 80 }.asReversed().take(50)
                        } else {
                            val boundary =
                                before?.let { position ->
                                    raw.indexOfFirst { it.position.deliveryCursor == position.deliveryCursor }
                                } ?: -1
                            retained.filter { raw.indexOf(it) > boundary }.take(50)
                        }
                    HistoryQueryPage(
                        selected,
                        selected.firstOrNull()?.position,
                        selected.lastOrNull()?.position,
                        true,
                        0u,
                    )
                }.around(saved)
            assertEquals(2, requests.size)
            assertEquals(raw[79].position.deliveryCursor, requests[1].first?.deliveryCursor)
            assertEquals(retained.filter { raw.indexOf(it) in 30..130 }, window.rows)
            assertEquals(
                raw[79].id,
                window.rows
                    .take(window.newerCount)
                    .last()
                    .id,
            )
            assertEquals(
                raw[81].id,
                window.rows
                    .drop(window.newerCount)
                    .first()
                    .id,
            )
        }

    @Test fun sdkCacheKeepsOnlyThreeTranscriptsAndUsesRetainedBoundariesAfterTrim() {
        val raw = rows(600)
        val cache = SDKTranscriptCache<Stored>({ it.id }, { it.position })
        val page = HistoryWindow(raw.flatMap { listOf(it, it) }, raw.first().position, raw.last().position, false, true)
        val kept = cache.put("a", page, raw[300].id)
        assertEquals(raw.subList(50, 550), kept.rows)
        assertEquals(raw[549].position.deliveryCursor, kept.last!!.deliveryCursor)
        assertTrue(kept.hasOlder)
        assertFalse(kept.atNewest)
        cache.put("b", page, null)
        cache.put("c", page, null)
        cache.get("a")
        cache.put("d", page, null)
        assertNull(cache.get("b"))
        assertNotNull(cache.get("a"))
        assertNotNull(cache.get("c"))
        assertNotNull(cache.get("d"))
    }
}
