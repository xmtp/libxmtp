package uniffi.xmtp_sdk

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.atomic.AtomicInteger

// SDKClient.events, startListener and stopListener in events/SDKEvents.kt are
// hand-written Kotlin around the native reader and listener. Rust tests own the
// native rules: xmtp_sdk/src/tests/event_readers.rs and event_listeners.rs.
class EventListenerTest {
    private val joinedFilter =
        EventFilter(kinds = listOf(EventKind.CONVERSATION_JOINED), groupIds = null, contentTypes = null)

    private val joined =
        ClientEvent.ConversationJoined(
            ConversationJoined(byteArrayOf(1), EventConversationType.GROUP, JoinOrigin.CREATED, "adder-inbox"),
        )

    /** A native client that keeps each listener so the test can call it. */
    private class ListenerClient : Client(NoHandle) {
        val listeners = mutableMapOf<ListenerId, EventListener>()
        val stopped = mutableListOf<ListenerId>()
        var ended = false

        override fun clientKey(): ULong = 1uL

        override suspend fun startListener(
            filter: EventFilter,
            listener: EventListener,
        ): ListenerId = (listeners.size + 1).toULong().also { listeners[it] = listener }

        override suspend fun stopListener(id: ListenerId) {
            stopped += id
        }

        override suspend fun end() {
            ended = true
        }
    }

    private fun sdkClient(raw: Client): SDKClient {
        val constructor = SDKClient::class.java.getDeclaredConstructor(Client::class.java, List::class.java)
        constructor.isAccessible = true
        return constructor.newInstance(raw, emptyList<ContentCodec<*>>())
    }

    // verifies: EVENT-053
    // A native callback that starts after stopListener returns does not reach the app.
    @Test
    fun callbackAfterStopDoesNotReachTheHandler() =
        runBlocking {
            val raw = ListenerClient()
            val client = sdkClient(raw)
            val calls = AtomicInteger()
            val id = client.startListener(joinedFilter) { calls.incrementAndGet() }
            val native = raw.listeners.getValue(id)
            native.onEvent(joined)
            assertEquals(1, calls.get())
            client.stopListener(id)
            assertEquals(listOf(id), raw.stopped)
            native.onEvent(joined)
            assertEquals("A callback ran after stopListener returned", 1, calls.get())

            val later = client.startListener(joinedFilter) { calls.incrementAndGet() }
            client.end()
            assertTrue(raw.ended)
            raw.listeners.getValue(later).onEvent(joined)
            assertEquals("A callback ran after end returned", 1, calls.get())
        }

    @Test
    fun handlerFailureIsListenerFailedAndCancellationPassesThrough() =
        runBlocking {
            val raw = ListenerClient()
            val client = sdkClient(raw)
            val secret = "listener-handler-secret"
            val failing = raw.listeners.getValue(client.startListener(joinedFilter) { throw LinkageError(secret) })
            val failure = runCatching { failing.onEvent(joined) }.exceptionOrNull()
            assertTrue("Expected ListenerException.Failed, got $failure", failure is ListenerException.Failed)
            assertTrue(
                "The listener failure kept the handler's error",
                generateSequence(failure) { it.cause }.none { it is LinkageError || it.toString().contains(secret) },
            )
            val cancellation = CancellationException("handler cancelled")
            val cancelling = raw.listeners.getValue(client.startListener(joinedFilter) { throw cancellation })
            // Coroutine stack recovery can copy the exception, so compare its type and text.
            val cancelled = runCatching { cancelling.onEvent(joined) }.exceptionOrNull()
            assertTrue("Cancellation became $cancelled", cancelled is CancellationException)
            assertEquals(cancellation.message, cancelled?.message)
        }

    // verifies: EVENT-014, EVENT-050
    @Test
    fun eventFlowAndListenerReceiveALiveEvent() =
        runBlocking {
            withTimeout(60_000) {
                withClients {
                    val client = create()
                    val events = client.events(joinedFilter)
                    val received = CompletableDeferred<ClientEvent>()
                    val id = client.startListener(joinedFilter) { received.complete(it) }
                    val first = async { events.first() }
                    val group = client.conversations.createGroup(emptyList())
                    val streamed = first.await() as ClientEvent.ConversationJoined
                    val heard = received.await() as ClientEvent.ConversationJoined
                    client.stopListener(id)
                    for (event in listOf(streamed, heard)) {
                        assertEquals(EventConversationType.GROUP, event.conversationJoined.conversationType)
                        assertEquals(group.id(), event.conversationJoined.groupId.toHex())
                    }
                }
            }
        }

    // verifies: EVENT-052
    // A running listener callback can stop its own listener and end its client.
    // Neither call waits for the callback to return.
    @Test
    fun runningCallbackStopsItsListenerAndEndsItsClient() =
        runBlocking {
            withTimeout(60_000) {
                withClients {
                    val client = create()
                    val stopped = CompletableDeferred<Unit>()
                    val ended = CompletableDeferred<Unit>()
                    val release = CompletableDeferred<Unit>()
                    val returned = CompletableDeferred<Throwable?>()
                    val calls = AtomicInteger()
                    val id = CompletableDeferred<ListenerId>()
                    id.complete(
                        client.startListener(joinedFilter) {
                            if (calls.incrementAndGet() > 1) return@startListener
                            val failure =
                                runCatching {
                                    client.stopListener(id.await())
                                    stopped.complete(Unit)
                                    client.end()
                                    ended.complete(Unit)
                                }.exceptionOrNull()
                            release.await()
                            returned.complete(failure)
                        },
                    )
                    // The callback can end the client before createGroup returns.
                    withContext(Dispatchers.Default) { runCatching { client.conversations.createGroup(emptyList()) } }
                    try {
                        assertTrue(
                            "stopListener inside the callback waited for the callback to return",
                            runCatching { withTimeout(10_000) { stopped.await() } }.isSuccess,
                        )
                        assertTrue(
                            "end inside the callback waited for the callback to return",
                            runCatching { withTimeout(10_000) { ended.await() } }.isSuccess,
                        )
                    } finally {
                        release.complete(Unit)
                    }
                    assertNull(withTimeout(10_000) { returned.await() })
                    assertEquals(1, calls.get())
                }
            }
        }

    private fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }
}
