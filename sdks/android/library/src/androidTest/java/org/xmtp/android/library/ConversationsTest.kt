package org.xmtp.android.library

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import uniffi.xmtp_sdk.*

class ConversationsTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private val alix get() = fixtures.alixClient
    private val bo get() = fixtures.boClient
    private val caro get() = fixtures.caroClient

    @Before override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
    }

    private fun id(value: Conversation): String =
        when (value) {
            is Conversation.Group -> value.group.id()
            is Conversation.Dm -> value.dm.id()
        }

    private fun topic(value: Conversation): String =
        when (value) {
            is Conversation.Group -> value.group.topic()
            is Conversation.Dm -> value.dm.topic()
        }

    @Test fun testsCanListConversations() =
        runBlocking {
            bo.conversations().createDm(caro.inboxId())
            bo.conversations().createGroup(listOf(caro.inboxId()))
            assertEquals(2, bo.conversations().list().size)
            assertEquals(1, bo.conversations().listDms(null).size)
            assertEquals(1, bo.conversations().listGroups(null).size)
            caro.conversations().sync()
            assertEquals(2, caro.conversations().list().size)
            assertEquals(1, caro.conversations().listGroups(null).size)
        }

    @Test fun testCanStreamAllMessages() =
        runBlocking {
            val group = caro.conversations().createGroup(listOf(bo.inboxId()))
            val dm = bo.conversations().createDm(caro.inboxId())
            bo.conversations().syncAll(null)
            val messages = StreamTestMessages()
            val job = launch(Dispatchers.IO) { bo.messages().collect { messages.add(it) } }
            try {
                messages.awaitHistory(bo.conversations().messageHistorySnapshot(10u).messages)
                val expected = listOf(group.sendText("hi") to "hi", dm.sendText("hi") to "hi")
                messages.awaitApplicationsAcrossConversations(expected) {
                    bo.conversations().messageHistorySnapshot(10u).messages
                }
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    @Test fun testCanStreamGroupsAndConversations() =
        runBlocking {
            val received = mutableListOf<Conversation>()
            val job =
                launch(Dispatchers.IO) {
                    bo.conversationStream().collect { synchronized(received) { received.add(it) } }
                }
            try {
                val group = caro.conversations().createGroup(listOf(bo.inboxId()))
                val dm = bo.conversations().createDm(caro.inboxId())
                withTimeout(10_000) { while (synchronized(received) { received.size } < 2) delay(10) }
                val snapshot = synchronized(received) { received.toList() }
                assertEquals(2, snapshot.size)
                assertEquals(setOf(group.id(), dm.id()), snapshot.map(::id).toSet())
                assertEquals(setOf(group.topic(), dm.topic()), snapshot.map(::topic).toSet())
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    @Test fun testCanStreamMessageDeletions() =
        runBlocking {
            val deleted = mutableSetOf<String>()
            val listener =
                bo.startListener(
                    EventFilter(listOf(EventKind.MESSAGE_EXPIRED), null, null, false),
                    { event ->
                        if (event is ClientEvent.MessageExpired) {
                            synchronized(
                                deleted,
                            ) { deleted.add(event.messageExpired.messageId.toHex()) }
                        }
                    },
                )
            try {
                val dm =
                    bo.conversations().createDm(
                        alix.inboxId(),
                        CreateDmOptions(DisappearingSettings(Timestamp(1_000_000_000), 1_000_000_000)),
                    )
                val id = dm.sendText("This message will disappear")
                withTimeout(10_000) { while (!synchronized(deleted) { id in deleted }) delay(10) }
                assertTrue(synchronized(deleted) { id in deleted })
                assertNull(bo.conversations().getMessageById(id))
            } finally {
                withContext(NonCancellable) { bo.stopListener(listener) }
            }
        }
}
