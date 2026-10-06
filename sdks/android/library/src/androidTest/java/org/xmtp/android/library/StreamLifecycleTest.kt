package org.xmtp.android.library

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.xmtp_sdk.*
import java.security.SecureRandom

class StreamLifecycleTest {
    @get:Rule val directories = TemporaryFolder()

    /** Catch-up stores pending history and leaves no work for a second call. */
    @Test fun testCatchUpToLiveColdCatchesPendingGroupAndIsIdempotent() =
        runBlocking {
            val previous = AndroidStreamLifecycle.enabled
            AndroidStreamLifecycle.enabled = false
            try {
                resumeStreams()
                val context = InstrumentationRegistry.getInstrumentation().targetContext
                val key = SecureRandom().generateSeed(32)

                fun options() =
                    ClientOptions(
                        backend = BackendSource.Options(localApi()),
                        storage =
                            StorageOptions(
                                StorageLocation.Directory(directories.newFolder().absolutePath),
                                encryptionKey = key,
                            ),
                        deviceSync = false,
                    )
                val sender = SDKClient.create(context, generateLocalSigner(), options())
                val receiver = SDKClient.create(context, generateLocalSigner(), options())
                try {
                    val group = sender.conversations.createGroup(listOf(receiver.inboxId()))
                    group.sendText("missed while away")
                    assertEquals(
                        "The receiver has no group before catch-up",
                        0,
                        receiver.conversations.listGroups(null).size,
                    )
                    val summary = receiver.catchUpToLive(null)
                    assertTrue(summary.completed)
                    assertEquals(0uL, summary.failed)
                    assertEquals(1uL, summary.conversations)
                    assertTrue(summary.messages >= 1uL)
                    val groups = receiver.conversations.listGroups(null)
                    assertEquals(1, groups.size)
                    val texts =
                        groups.single().messages().mapNotNull {
                            ((it.content as? SDKMessageContent.Standard)?.value as? MessageContent.Text)?.v1
                        }
                    assertTrue(
                        "Catch-up must store the missed text without another sync",
                        texts.contains("missed while away"),
                    )
                    val again = receiver.catchUpToLive(null)
                    assertTrue(again.completed)
                    assertEquals(0uL, again.failed)
                    assertEquals(0uL, again.messages)
                    assertEquals(0uL, again.conversations)
                } finally {
                    withContext(NonCancellable) {
                        receiver.end()
                        sender.end()
                    }
                }
            } finally {
                AndroidStreamLifecycle.enabled = previous
            }
        }

    @Test fun testManageStreamLifecycleDefaultsOn() {
        assertTrue(AndroidStreamLifecycle.enabled)
    }
}
