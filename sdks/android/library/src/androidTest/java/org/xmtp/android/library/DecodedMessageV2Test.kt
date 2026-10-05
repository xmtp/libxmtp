package org.xmtp.android.library

import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class DecodedMessageV2Test : BaseInstrumentedTest() {
    private fun text(message: Message): String? =
        ((message.content as? SDKMessageContent.Standard)?.value as? MessageContent.Text)?.v1

    private suspend fun group(): Triple<TestFixtures, Group, Group> {
        val fixtures = createFixtures()
        val boGroup = fixtures.boClient.conversations().createGroup(listOf(fixtures.alixClient.inboxId()))
        fixtures.alixClient.conversations().syncAll(null)
        val alixGroup =
            (
                checkNotNull(
                    fixtures.alixClient.conversations().getById(boGroup.id()),
                ) as Conversation.Group
            ).group
        return Triple(fixtures, boGroup, alixGroup)
    }

    private fun assertThreeTexts(messages: List<Message>) {
        assertEquals(4, messages.size)
        assertEquals(listOf("Second message from Bo", "Hello from Alix", "Hello from Bo"), messages.take(3).map(::text))
        assertTrue(messages.last().data.content is MessageContent.GroupUpdated)
    }

    @Test fun testCanRetrieveEnrichedMessagesFromGroup() =
        runBlocking {
            val (_, bo, alix) = group()
            bo.sendText("Hello from Bo")
            alix.sendText("Hello from Alix")
            bo.sendText("Second message from Bo")
            bo.sync()
            assertThreeTexts(bo.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)))
        }

    @Test fun testCanRetrieveMessagesV2FromDm() =
        runBlocking {
            val fixtures = createFixtures()
            val bo = fixtures.boClient.conversations().createDm(fixtures.alixClient.inboxId())
            fixtures.alixClient.conversations().syncAll(null)
            val alix = checkNotNull(fixtures.alixClient.conversations().getDmByInboxId(fixtures.boClient.inboxId()))
            bo.sendText("Hello from Bo")
            alix.sendText("Hello from Alix")
            bo.sendText("Second message from Bo")
            bo.sync()
            assertThreeTexts(bo.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)))
        }

    @Test fun testMessagesV2Pagination() =
        runBlocking {
            val (_, bo, _) = group()
            for (i in 1..10) bo.sendText("Message $i from Bo")
            val limited = bo.messages(ListMessagesOptions(limit = 5u))
            assertEquals(5, limited.size)
            val boundary = limited[2].sentAt
            val before = bo.messages(ListMessagesOptions(sentBefore = boundary))
            val after = bo.messages(ListMessagesOptions(sentAfter = boundary))
            assertTrue(before.isNotEmpty())
            assertTrue(after.isNotEmpty())
            assertTrue(before.all { it.sentAt.ns < boundary.ns })
            assertTrue(after.all { it.sentAt.ns > boundary.ns })
            assertFalse(before.any { it.id == limited[2].id })
            assertFalse(after.any { it.id == limited[2].id })
        }

    @Test fun testMessagesV2SortDirection() =
        runBlocking {
            val (_, bo, _) = group()
            for (body in listOf("First message", "Second message", "Third message")) {
                bo.sendText(body)
                delay(100)
            }
            val descending = bo.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING))
            val ascending = bo.messages(ListMessagesOptions(direction = MessageOrder.ASCENDING))
            assertEquals(listOf("Third message", "Second message", "First message"), descending.take(3).map(::text))
            assertTrue(descending.last().data.content is MessageContent.GroupUpdated)
            assertTrue(ascending.first().data.content is MessageContent.GroupUpdated)
            assertEquals(listOf("First message", "Second message", "Third message"), ascending.drop(1).map(::text))
            assertEquals(descending.map { it.id }.reversed(), ascending.map { it.id })
        }

    @Test fun testMessagesV2DeliveryStatus() =
        runBlocking {
            val (_, bo, _) = group()
            val publishedId = bo.sendText("Published message")
            val unpublishedId = bo.prepareMessage(encodeText("Unpublished message"))
            assertEquals(3, bo.messages().size)
            val published = bo.messages(ListMessagesOptions(deliveryStatus = DeliveryStatus.PUBLISHED))
            assertEquals(2, published.size)
            assertEquals("Published message", text(published.single { it.id == publishedId }))
            assertFalse(published.any { it.id == unpublishedId })
            val unpublished = bo.messages(ListMessagesOptions(deliveryStatus = DeliveryStatus.UNPUBLISHED))
            assertEquals(unpublishedId, unpublished.single().id)
            assertEquals("Unpublished message", text(unpublished.single()))
        }

    @Test fun testMessagesV2CustomContentTypes() =
        runBlocking {
            val codec = NumberCodec()
            val alix = createClient(createWallet(), codecs = listOf(codec))
            val bo = createClient(createWallet(), codecs = listOf(codec))
            val group = alix.conversations().createGroup(listOf(bo.inboxId()))
            val id = group.send(codec, 3.14)
            val message = group.messages().single { it.id == id }
            assertEquals(3.14, (message.content as SDKMessageContent.Custom).value)
            assertEquals(codec.type, message.contentType)
            assertNull((message.content as SDKMessageContent.Custom).error)
        }

    @Test fun testMessagesV2IncludeReactions() =
        runBlocking {
            val (fixtures, bo, alix) = group()
            val id = bo.sendText("Hello with reactions")
            alix.sync()
            alix.sendReaction(
                id,
                fixtures.boClient.inboxId(),
                Reaction("👍", ReactionAction.ADDED, ReactionSchema.UNICODE),
            )
            bo.sendReaction(
                id,
                fixtures.boClient.inboxId(),
                Reaction("❤️", ReactionAction.ADDED, ReactionSchema.UNICODE),
            )
            bo.sync()
            val parent = bo.messages().single { it.id == id }
            assertEquals(2, parent.reactions.size)
            assertEquals(setOf("❤️", "👍"), parent.reactions.map { it.reaction.content }.toSet())
            assertEquals(
                setOf(fixtures.alixClient.inboxId(), fixtures.boClient.inboxId()),
                parent.reactions
                    .map {
                        it.senderInboxId
                    }.toSet(),
            )
        }

    @Test fun testReactionCountAccuracy() =
        runBlocking {
            val (fixtures, bo, alix) = group()
            val id = bo.sendText("Test reaction count")
            alix.sync()
            for (i in 1..5) {
                alix.sendReaction(
                    id,
                    fixtures.boClient.inboxId(),
                    Reaction("emoji$i", ReactionAction.ADDED, ReactionSchema.UNICODE),
                )
            }
            bo.sync()
            val parent = bo.messages().single { it.id == id }
            assertEquals(5, parent.reactions.size)
            assertEquals((1..5).map { "emoji$it" }.toSet(), parent.reactions.map { it.reaction.content }.toSet())
        }

    @Test fun testLeaveRequestMessageIsDecodedProperly() =
        runBlocking {
            val (fixtures, bo, alix) = group()
            alix.requestRemoval()
            bo.sync()
            val leave =
                bo.messages().single {
                    it.senderInboxId == fixtures.alixClient.inboxId() && it.data.content is MessageContent.LeaveRequest
                }
            assertNotNull((leave.content as SDKMessageContent.Standard).value as MessageContent.LeaveRequest)
        }
}
