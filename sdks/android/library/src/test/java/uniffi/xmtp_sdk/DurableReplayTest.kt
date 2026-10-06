package uniffi.xmtp_sdk

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test
import java.nio.file.Files

// The Flow adapter in streams/Readers.kt and the native reader together, on a
// persistent database. MessageStreamTest and MessageDeliveryFlowTest prove the
// host call order with fakes; xmtp_sdk/src/tests/reader_ack_cancellation.rs
// proves the native reader. This test proves that a Flow collection that is
// cancelled, or whose collector throws, leaves the item it holds eligible after
// the client ends and is built again, and that a completed handoff is not
// replayed.
class DurableReplayTest {
    // verifies: PROC-052, PROC-041
    @Test
    fun collectorBoundarySurvivesEndAndRebuild() =
        runBlocking {
            val directory = Files.createTempDirectory("android-durable-replay-")
            try {
                withTimeout(60_000) {
                    withClients {
                        val database = directory.resolve("client.db3").toString()
                        val options =
                            liveOptions().copy(
                                storage =
                                    StorageOptions(
                                        location = StorageLocation.Explicit(database, "$database-attachments"),
                                    ),
                            )
                        val signer = generateLocalSigner()
                        val identity = signer.identity()
                        var owner = create(signer, options)
                        val inbox = owner.inboxId()
                        val group = owner.conversations().createGroup(emptyList())
                        val first = group.sendText("held collector A")
                        val second = group.sendText("held collector B")

                        suspend fun rebuild(): Group {
                            withContext(NonCancellable) { owner.end() }
                            owner = build(identity, options, inbox)
                            return (checkNotNull(owner.conversations().getById(group.id())) as Conversation.Group).group
                        }

                        // `first()` leaves the reader early, so it does not acknowledge.
                        suspend fun replayed(conversation: Group): MessageId? =
                            withTimeoutOrNull(10_000) { owner.messages(conversation).first().id }

                        // Cancel the collection while its collector holds A.
                        val entered = CompletableDeferred<Unit>()
                        val delivered = mutableListOf<MessageId>()
                        val closes = mutableListOf<SDKStreamCloseReason>()
                        val collection =
                            async {
                                owner.messages(group, onClose = { closes.add(it) }).collect {
                                    delivered.add(it.id)
                                    entered.complete(Unit)
                                    awaitCancellation()
                                }
                            }
                        try {
                            entered.await()
                        } finally {
                            withContext(NonCancellable) { collection.cancelAndJoin() }
                        }
                        assertEquals(listOf(first), delivered)
                        assertEquals(listOf<SDKStreamCloseReason>(SDKStreamCloseReason.Closed), closes)
                        val restored = rebuild()
                        assertEquals("The cancelled collector lost A", first, replayed(restored))

                        // Complete A, then throw from the collector while it holds B.
                        val failure = AssertionError("collector stops on B")
                        val consumed = mutableListOf<MessageId>()
                        val failedCloses = mutableListOf<SDKStreamCloseReason>()
                        val thrown =
                            runCatching {
                                owner.messages(restored, onClose = { failedCloses.add(it) }).collect {
                                    consumed.add(it.id)
                                    if (it.id == second) throw failure
                                }
                            }.exceptionOrNull()
                        assertSame(failure, thrown)
                        assertEquals(listOf(first, second), consumed)
                        assertEquals(listOf<SDKStreamCloseReason>(SDKStreamCloseReason.Closed), failedCloses)
                        assertEquals("The failed collector lost B or replayed A", second, replayed(rebuild()))
                    }
                }
            } finally {
                directory.toFile().deleteRecursively()
            }
        }
}
