package org.xmtp.android.library

import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.collect
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import uniffi.xmtp_sdk.*

class GroupTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private val alix get() = fixtures.alixClient
    private val bo get() = fixtures.boClient
    private val caro get() = fixtures.caroClient

    @Before override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
    }

    private suspend fun group() = bo.conversations().createGroup(listOf(alix.inboxId()))

    private suspend fun find(
        client: SDKClient,
        id: ConversationId,
    ): Group = (checkNotNull(client.conversations().getById(id)) as Conversation.Group).group

    private fun text(message: Message): String? =
        ((message.content as? SDKMessageContent.Standard)?.value as? MessageContent.Text)?.v1

    @Test fun testCanSendMessageToGroup() =
        runBlocking {
            val group = group()
            group.sendText("howdy")
            val id = group.sendText("gm")
            group.sync()
            assertEquals("gm", text(group.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first()))
            assertEquals(id, group.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first().id)
            assertEquals(
                DeliveryStatus.PUBLISHED,
                group.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first().deliveryStatus,
            )
            assertEquals(3, group.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).size)
            alix.conversations().sync()
            val peer = find(alix, group.id())
            peer.sync()
            assertEquals(3, peer.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).size)
            assertEquals("gm", text(peer.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first()))
        }

    @Test fun testCanStreamGroupMessages() =
        runBlocking {
            val group = group()
            alix.conversations().sync()
            val peer = find(alix, group.id())
            val retained = group.messageHistorySnapshot(10u).messages
            assertEquals(1, retained.size)
            assertEquals(MessageKind.MEMBERSHIP_CHANGE, retained.single().kind)
            val messages = StreamTestMessages()
            val job = launch(Dispatchers.IO) { bo.messages(group).collect { messages.add(it) } }
            try {
                messages.awaitHistory(retained)
                val first = peer.sendText("hi")
                messages.awaitApplications(listOf(first to "hi"))
                try {
                    peer.send(EncodedContent(GroupUpdatedCodec().type, content = byteArrayOf()))
                    fail("Applications cannot send reserved membership content")
                } catch (error: XmtpException.InvalidInput) {
                    assertEquals("ReservedTranscriptContentType", error.v1.code)
                    assertEquals(ErrorCategory.INPUT, error.v1.category)
                    assertFalse(error.v1.retryable)
                }
                val second = peer.sendText("hi again")
                messages.awaitApplications(listOf(first to "hi", second to "hi again"))
                val history = group.messages()
                assertEquals(3, history.size)
                messages.awaitHistory(history)
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    @Test fun testCanStreamGroups() =
        runBlocking {
            val reader =
                alix.conversations().conversationReader(ConversationReaderOptions(kind = ConversationKind.GROUP))
            val received = Channel<Pair<ConversationId, String>>(Channel.UNLIMITED)
            val closed = CompletableDeferred<Unit>()
            val job =
                launch(Dispatchers.IO) {
                    try {
                        while (true) {
                            val value = reader.next() ?: break
                            val row =
                                when (value) {
                                    is Conversation.Group -> value.group.id() to value.group.topic()
                                    is Conversation.Dm -> value.dm.id() to value.dm.topic()
                                }
                            received.send(row)
                        }
                    } finally {
                        withContext(NonCancellable) { reader.end() }
                        closed.complete(Unit)
                    }
                }
            try {
                val first = bo.conversations().createGroup(listOf(alix.inboxId()))
                val second = caro.conversations().createGroup(listOf(alix.inboxId()))
                val expected = setOf(first.id() to first.topic(), second.id() to second.topic())
                // Welcomes from two senders can arrive in either order; the stream
                // orders rows by the time the Welcome was processed. Two received
                // rows that equal two distinct expected rows means each arrived once.
                val actual = List(2) { withTimeout(3000) { received.receive() } }
                assertEquals(expected, actual.toSet())
                assertTrue("Unexpected conversation", received.tryReceive().isFailure)
            } finally {
                withContext(NonCancellable) {
                    withTimeout(30_000) {
                        job.cancelAndJoin()
                        closed.await()
                    }
                    received.cancel()
                }
            }
        }
}
