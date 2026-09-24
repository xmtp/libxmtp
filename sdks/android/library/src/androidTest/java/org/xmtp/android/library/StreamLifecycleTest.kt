package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class StreamLifecycleTest {
    /**
     * One-shot catch-up joins a pending group and replays its history from
     * durable cursors, then stops — and a second call finds nothing owed.
     */
    @Test
    fun testCatchUpToLiveColdCatchesPendingGroupAndIsIdempotent() {
        val fixtures = fixtures()
        runBlocking {
            // bo creates a group with alix and sends. alix has never synced, so
            // both the welcome and the message are owed.
            val boGroup =
                fixtures.boClient.conversations.newGroup(listOf(fixtures.alixClient.inboxId))
            boGroup.send("missed while away")

            assertEquals(
                0,
                fixtures.alixClient.conversations
                    .listGroups()
                    .size,
            )

            val summary = fixtures.alixClient.catchUpToLive()
            assertTrue(summary.completed)
            assertEquals(1L, summary.conversations)
            assertTrue(summary.messages >= 1)

            val groups = fixtures.alixClient.conversations.listGroups()
            assertEquals(1, groups.size)

            // Catch-up delivered the real payload to the local store, not just a
            // counter: the missed text is readable with no further sync.
            val texts = groups.first().messages().map { it.body }
            assertTrue(
                "the missed message must be stored and readable after catch-up",
                texts.contains("missed while away"),
            )

            // Nothing owed now: a second run persists nothing new on either axis.
            val again = fixtures.alixClient.catchUpToLive()
            assertEquals(0L, again.messages)
            assertEquals(0L, again.conversations)
        }
    }

    /** Backgrounding auto-management is a process-global toggle, on by default. */
    @Test
    fun testManageStreamLifecycleDefaultsOn() {
        assertTrue(Client.manageStreamLifecycle)
    }
}
