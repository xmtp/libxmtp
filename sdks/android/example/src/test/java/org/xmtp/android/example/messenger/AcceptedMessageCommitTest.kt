package org.xmtp.android.example.messenger

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import java.io.IOException

class AcceptedMessageCommitTest {
    @Test fun failedPersistenceKeepsTheSameIdUntilVerifiedAcknowledgment() =
        runBlocking {
            val steps = mutableListOf<String>()
            var fail = true
            val commit =
                AcceptedMessageCommit(
                    "retained-id",
                    persist = {
                        steps += "persist"
                        if (fail) throw IOException("App commit failed")
                        true
                    },
                    verify = {
                        steps += "verify"
                        true
                    },
                    acknowledge = { steps += "acknowledge" },
                )
            assertThrows(IOException::class.java) { runBlocking { commit.finish() } }
            assertEquals(listOf("persist"), steps)
            assertEquals("retained-id", commit.messageId)
            fail = false
            assertTrue(commit.finish())
            assertEquals(listOf("persist", "persist", "verify", "acknowledge"), steps)
            assertTrue(commit.finish())
            assertEquals("Acknowledgment occurs once", 4, steps.size)
        }

    @Test fun failedReadbackDoesNotAcknowledgeAnUnverifiedReference() =
        runBlocking {
            var verified = false
            var acknowledged = 0
            val commit =
                AcceptedMessageCommit(
                    "retained-id",
                    persist = { true },
                    verify = { verified },
                    acknowledge = { acknowledged += 1 },
                )
            assertThrows(IllegalStateException::class.java) { runBlocking { commit.finish() } }
            assertEquals(0, acknowledged)
            verified = true
            assertTrue(commit.finish())
            assertEquals(1, acknowledged)
        }
}
