package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.xmtp_sdk.*

@RunWith(AndroidJUnit4::class)
class MessageComparisonTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var boGroup: Group
    private lateinit var alixGroup: Group
    private val options = ListMessagesOptions(limit = 100u, direction = MessageOrder.ASCENDING)

    @Before override fun setUp() {
        super.setUp()
        runBlocking {
            fixtures = createFixtures()
            boGroup = fixtures.boClient.conversations().createGroup(listOf(fixtures.alixClient.inboxId()))
            fixtures.alixClient.conversations().sync()
            alixGroup =
                fixtures.alixClient
                    .conversations()
                    .listGroups(null)
                    .single()
        }
    }

    private suspend fun sync() {
        boGroup.sync()
        alixGroup.sync()
    }

    private fun text(message: Message): String? =
        ((message.content as? SDKMessageContent.Standard)?.value as? MessageContent.Text)?.v1

    private fun reaction(message: Message): MessageContent.Reaction? =
        (message.content as? SDKMessageContent.Standard)?.value as? MessageContent.Reaction

    private suspend fun replay(count: Int): List<Message> {
        val reader =
            boGroup.messageReader(
                ConversationMessageReaderOptions(
                    from = fixtures.boClient.conversations().beginningDeliveryCursor(),
                ),
            )
        return try {
            withTimeout(10_000) { List(count) { checkNotNull(reader.next()) } }
        } finally {
            withContext(NonCancellable) { reader.end() }
        }
    }

    private fun compare(
        stored: List<Message>,
        streamed: List<Message>,
    ) {
        assertEquals(stored.map { it.id }, streamed.map { it.id })
        for ((first, second) in stored.zip(streamed)) {
            assertEquals(first.senderInboxId, second.senderInboxId)
            assertEquals(first.conversationId, second.conversationId)
            assertEquals(first.sentAt, second.sentAt)
            assertEquals(first.kind, second.kind)
            assertEquals(first.data.content, second.data.content)
            assertEquals(first.reactions, second.reactions)
            assertArrayEquals(first.rawBytes, second.rawBytes)
        }
    }

    @Test fun testV1VsV2MessageCount() =
        runBlocking {
            boGroup.sendText("Message 1")
            alixGroup.sendText("Message 2")
            boGroup.sendText("Message 3")
            val parent = boGroup.sendText("Message with reaction")
            sync()
            alixGroup.sendReaction(
                parent,
                fixtures.boClient.inboxId(),
                Reaction("👍", ReactionAction.ADDED, ReactionSchema.UNICODE),
            )
            sync()
            val stored = boGroup.messageHistorySnapshot(100u).messages
            val streamed = replay(stored.size)
            assertEquals(4, stored.count { text(it) != null })
            assertEquals(4, streamed.count { text(it) != null })
            assertEquals(1, stored.count { reaction(it) != null })
            compare(stored, streamed)
        }

    @Test fun testV1VsV2ContentEquality() =
        runBlocking {
            val expected =
                listOf(
                    "First message",
                    "Second message",
                    "Third message with emoji 🎉",
                    "Fourth message with special chars !@#$%",
                )
            val expectedIds = expected.map { boGroup.sendText(it) }
            sync()
            val stored = boGroup.messageHistorySnapshot(100u).messages
            val streamed = replay(stored.size)
            val peer = alixGroup.messages(options)
            assertEquals(expected, stored.mapNotNull(::text))
            assertEquals(expected, streamed.mapNotNull(::text))
            assertEquals(expected, peer.mapNotNull(::text))
            compare(stored, streamed)
            assertEquals(expectedIds, peer.filter { it.id in expectedIds }.map { it.id })
            compare(stored.filter { it.id in expectedIds }, peer.filter { it.id in expectedIds })
        }

    @Test fun testPerformanceComparison() =
        runBlocking {
            for (i in 1..20) {
                val parent = boGroup.sendText("Message $i")
                if (i % 5 == 0) {
                    sync()
                    alixGroup.sendReaction(
                        parent,
                        fixtures.boClient.inboxId(),
                        Reaction("👍", ReactionAction.ADDED, ReactionSchema.UNICODE),
                    )
                }
            }
            sync()
            val historyStart = System.nanoTime()
            val stored = boGroup.messageHistorySnapshot(100u).messages
            val historyNs = System.nanoTime() - historyStart
            val replayStart = System.nanoTime()
            val streamed = replay(stored.size)
            val replayNs = System.nanoTime() - replayStart
            println("Stored history: $historyNs ns; reader replay: $replayNs ns; ${stored.size} messages")
            assertEquals(20, stored.count { text(it) != null })
            assertEquals(4, stored.count { it.reactions.isNotEmpty() })
            compare(stored, streamed)
        }

    @Test fun testV2ReactionsAreEmbedded() =
        runBlocking {
            val parent = boGroup.sendText("Message for reactions")
            sync()
            alixGroup.sendReaction(
                parent,
                fixtures.boClient.inboxId(),
                Reaction("👍", ReactionAction.ADDED, ReactionSchema.UNICODE),
            )
            boGroup.sendReaction(
                parent,
                fixtures.boClient.inboxId(),
                Reaction("❤️", ReactionAction.ADDED, ReactionSchema.UNICODE),
            )
            sync()
            val stored = boGroup.messageHistorySnapshot(100u).messages
            val streamed = replay(stored.size)
            assertEquals(2, stored.count { reaction(it) != null })
            assertEquals(
                setOf("👍", "❤️"),
                stored
                    .single { it.id == parent }
                    .reactions
                    .map { it.reaction.content }
                    .toSet(),
            )
            assertEquals(2, streamed.single { it.id == parent }.reactions.size)
            compare(stored, streamed)
        }
}
