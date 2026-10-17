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
        val text: String = "original",
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

    @Test fun anchoredRefreshKeepsBothLoadedSidesAndUpdatesTheQueriedRange() {
        val raw = rows(300)
        val cache = SDKTranscriptCache<Stored>({ it.id }, { it.position })
        cache.put("chat", HistoryWindow(raw, raw.first().position, raw.last().position, false, true), raw[150].id)
        val inserted = Stored("new-row", MessageHistoryPosition(Timestamp(10), "opaque-new"))
        val refreshed =
            raw
                .subList(100, 201)
                .filter { it.id != raw[140].id }
                .map {
                    if (it.id == raw[130].id) it.copy(text = "updated") else it
                }.toMutableList()
                .apply { add(20, inserted) }
        val kept =
            cache.refresh(
                "chat",
                HistoryWindow(refreshed, refreshed.first().position, refreshed.last().position, true, false),
                raw[150].id,
            )
        assertEquals(raw.take(100) + refreshed + raw.drop(201), kept.rows)
        assertEquals(raw.first().position, kept.first)
        assertEquals(raw.last().position, kept.last)
        assertTrue(kept.atNewest)
        assertFalse(kept.hasOlder)
        assertEquals("updated", kept.rows.single { it.id == raw[130].id }.text)
        assertFalse(kept.rows.any { it.id == raw[140].id })
    }

    @Test fun anchoredRefreshStillBoundsTheRetainedRangeAndKeepsItsContinuation() {
        val raw = rows(600)
        val cache = SDKTranscriptCache<Stored>({ it.id }, { it.position })
        val initial =
            cache.put(
                "chat",
                HistoryWindow(raw, raw.first().position, raw.last().position, false, true),
                raw[300].id,
            )
        val inserted = Stored("new-row", MessageHistoryPosition(Timestamp(10), "opaque-new"))
        val refreshed = raw.subList(250, 351).toMutableList().apply { add(20, inserted) }
        val kept =
            cache.refresh(
                "chat",
                HistoryWindow(refreshed, refreshed.first().position, refreshed.last().position, true, false),
                raw[300].id,
            )
        val merged = initial.rows.take(200) + refreshed + initial.rows.drop(301)
        assertEquals(merged.drop(1).take(500), kept.rows)
        assertEquals(500, kept.rows.size)
        assertEquals(raw[51].position, kept.first)
        assertEquals(raw[549].position, kept.last)
        assertTrue(kept.hasOlder)
        assertFalse(kept.atNewest)
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
