package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.PrivateKeyBuilder
import uniffi.xmtpv3.resumeStreams
import java.security.SecureRandom

@RunWith(AndroidJUnit4::class)
class StreamLifecycleTest {
    /**
     * One-shot catch-up joins a pending group and replays its history from
     * durable cursors, then stops — and a second call finds nothing owed.
     */
    @Test
    fun testCatchUpToLiveColdCatchesPendingGroupAndIsIdempotent() {
        val previousManageStreamLifecycle = Client.manageStreamLifecycle
        Client.manageStreamLifecycle = false
        try {
            runBlocking { resumeStreams() }
            runBlocking {
                val options =
                    ClientOptions(
                        api = localApi(),
                        dbEncryptionKey = SecureRandom().generateSeed(32),
                        appContext = InstrumentationRegistry.getInstrumentation().targetContext,
                        deviceSyncEnabled = false,
                    )
                val sender = Client.create(account = PrivateKeyBuilder(), options = options)
                val receiver = Client.create(account = PrivateKeyBuilder(), options = options)
                try {
                    // No device-sync worker can receive the Welcome before catch-up.
                    val boGroup =
                        sender.conversations.newGroup(listOf(receiver.inboxId))
                    boGroup.send("missed while away")

                    assertEquals(
                        "the receiver must have no group before catch-up",
                        0,
                        receiver.conversations
                            .listGroups()
                            .size,
                    )

                    val summary = receiver.catchUpToLive()
                    assertTrue(summary.completed)
                    assertEquals(0L, summary.failed)
                    assertEquals(1L, summary.conversations)
                    assertTrue(summary.messages >= 1)

                    val groups = receiver.conversations.listGroups()
                    assertEquals(1, groups.size)

                    // Catch-up delivered the real payload to the local store, not just a
                    // counter: the missed text is readable with no further sync.
                    val texts = groups.first().messages().map { it.body }
                    assertTrue(
                        "the missed message must be stored and readable after catch-up",
                        texts.contains("missed while away"),
                    )

                    // Nothing owed now: a second run persists nothing new on either axis.
                    val again = receiver.catchUpToLive()
                    assertTrue(again.completed)
                    assertEquals(0L, again.failed)
                    assertEquals(0L, again.messages)
                    assertEquals(0L, again.conversations)
                } finally {
                    receiver.deleteLocalDatabase()
                    sender.deleteLocalDatabase()
                }
            }
        } finally {
            Client.manageStreamLifecycle = previousManageStreamLifecycle
        }
    }

    /** Backgrounding auto-management is a process-global toggle, on by default. */
    @Test
    fun testManageStreamLifecycleDefaultsOn() {
        assertTrue(Client.manageStreamLifecycle)
    }
}
