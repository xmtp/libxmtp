package org.xmtp.android.example.messenger.attachments

import org.junit.Assert.*
import org.junit.Test

class VerifiedAttachmentCacheTest {
    @Test fun pressureKeepsOnlyTheBoundAndPreservesARecentlyUsedRecord() {
        val cache = VerifiedAttachmentCache<String>()
        val generation = cache.generation()
        repeat(VerifiedAttachmentCache.MAX_RECORDS) { id ->
            assertTrue(cache.admit(generation, "$id", "path-$id") { true })
        }
        assertEquals("path-0", cache["0"])
        assertTrue(cache.admit(generation, "new", "new-path") { true })
        assertEquals("path-0", cache["0"])
        assertNull(cache["1"])
        repeat(10_000) { id -> cache.admit(generation, "more-$id", "path-$id") { true } }
        assertEquals(VerifiedAttachmentCache.MAX_RECORDS, cache.retained.value.size)
    }

    @Test fun clearRejectsAnOldCompletionAndKeepsAnActionSnapshot() {
        val cache = VerifiedAttachmentCache<ByteArray>()
        val generation = cache.generation()
        val bytes = "verified bytes".toByteArray()
        assertTrue(cache.admit(generation, "open", bytes) { true })
        val action = checkNotNull(cache["open"])
        cache.clear()
        assertTrue(cache.retained.value.isEmpty())
        assertFalse(
            "A late completion cannot restore the cleared view",
            cache.admit(generation, "late", bytes) { true },
        )
        assertNull(cache["late"])
        assertArrayEquals(bytes, action)
        assertTrue(cache.admit(cache.generation(), "fresh", bytes) { true })
        assertEquals(setOf("fresh"), cache.retained.value)
    }

    @Test fun aStaleScreenCannotAdmitAResultBeforeItsClearCollectorRuns() {
        val cache = VerifiedAttachmentCache<String>()
        assertFalse(cache.admit(cache.generation(), "stale", "path") { false })
        assertTrue(cache.retained.value.isEmpty())
    }
}
