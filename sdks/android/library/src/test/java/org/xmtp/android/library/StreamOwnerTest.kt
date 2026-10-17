package org.xmtp.android.library

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*
import java.lang.ref.WeakReference

private class HeldFlow(
    var flow: Flow<Message>?,
)

class StreamOwnerTest {
    private fun collectGarbage() {
        repeat(10) {
            System.gc()
            Thread.yield()
        }
    }

    // The adapter must hold the host before collection and between collections.
    // The recording reader proves host retention and decode, not native ACK.
    @Test(timeout = 10_000)
    fun coldFlowKeepsOwnerBetweenCollections() {
        val releasedOwner =
            runBlocking {
                val type = ContentTypeId("tests.xmtp.org", "flow-owner", 1u, 0u)
                val encoded = EncodedContent(type, content = byteArrayOf(1))
                val codec =
                    object : ContentCodec<String> {
                        override val type = encoded.type

                        override fun encode(value: String) = encoded

                        override fun decode(encoded: EncodedContent) = "owner decode"
                    }
                var opens = 0
                lateinit var raw: RecordingReaderClient
                raw =
                    RecordingReaderClient {
                        opens++
                        RecordingMessageReader {
                            deliveryTestMessage(
                                content = MessageContent.Custom(encoded, byteArrayOf(1)),
                                encoded = encoded,
                                clientKey = raw.key,
                            )
                        }
                    }

                fun makeFlow(): Pair<HeldFlow, WeakReference<SDKClient>> {
                    val client = testSDKClient(raw, listOf(codec))
                    return HeldFlow(client.conversations.streamAllMessages()) to WeakReference(client)
                }
                val (held, owner) = makeFlow()
                collectGarbage()
                assertNotNull("The uncollected Flow lost its host", owner.get())
                assertEquals("Flow construction opened a reader", 0, opens)
                repeat(2) {
                    val item = held.flow!!.first()
                    assertEquals("owner decode", (item.content as SDKMessageContent.Custom).value)
                    assertEquals(it + 1, opens)
                    collectGarbage()
                    assertNotNull("The Flow lost its host between collections", owner.get())
                }
                held.flow = null
                owner
            }
        // Check release after the coroutine returns, so its saved local values
        // cannot keep the last collection's Flow alive.
        repeat(50) {
            collectGarbage()
            if (releasedOwner.get() == null) return@repeat
            Thread.sleep(10)
        }
        assertNull("The released Flow retained its host", releasedOwner.get())
    }

    @Test(timeout = 10_000)
    fun missingOwnerFailsOnCollectionWithoutOpening() =
        runBlocking {
            var opens = 0
            val raw =
                RecordingReaderClient {
                    opens++
                    RecordingMessageReader { null }
                }
            // No host has registered this binding receiver.
            val reasons = mutableListOf<SDKStreamCloseReason>()
            val flow = raw.conversations().streamAllMessages(MessageStreamOptions(onClose = reasons::add))
            assertEquals(0, opens)
            val error = runCatching { flow.first() }.exceptionOrNull()
            assertTrue(error is XmtpException.ClientClosed)
            assertEquals(0, opens)
            assertEquals(1, reasons.size)
            assertSame(error, (reasons.single() as SDKStreamCloseReason.Failed).error)
        }
}
