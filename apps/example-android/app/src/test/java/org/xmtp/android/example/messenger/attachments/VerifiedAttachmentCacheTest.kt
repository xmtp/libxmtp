package org.xmtp.android.example.messenger.attachments

import org.junit.Assert.*
import org.junit.Test

class VerifiedAttachmentCacheTest {
    @Test fun scrollingThroughAttachmentsKeepsEarlierFilesAvailable() {
        val cache = VerifiedAttachmentCache<String>()
        val generation = cache.generation()
        repeat(100) { id ->
            assertTrue(cache.admit(generation, "$id", "path-$id") { true })
        }
        repeat(100) { id ->
            assertEquals("path-$id", cache["$id"])
            assertTrue("A downloaded file must remain openable", "$id" in cache.retained.value)
        }
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
