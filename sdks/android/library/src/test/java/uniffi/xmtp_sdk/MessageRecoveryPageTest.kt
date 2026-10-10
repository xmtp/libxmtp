package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class MessageRecoveryPageTest {
    @Test
    fun nativeRecordsKeepMessagesAndBothPositionBounds() =
        runBlocking {
            withTimeout(60_000) {
                withClients {
                    val client = create()
                    val peer = create()
                    for (conversation in listOf(
                        Conversation.Group(client.conversations.createGroup(emptyList())),
                        Conversation.Dm(client.conversations.createDm(peer.inboxId())),
                    )) {
                        val ids =
                            listOf(
                                "first",
                                "second",
                                "third",
                            ).map { conversation.sendText(it, SendOptions(optimistic = true)) }
                        val first =
                            conversation.messageRecoveryPage(
                                ListMessagesOptions(limit = 2u, kind = MessageKind.APPLICATION),
                            )
                        assertEquals(ids.take(2), first.messages.map { it.id })
                        assertEquals(0u, first.skippedCount)
                        assertTrue(first.hasMore)
                        assertNotNull(first.firstPosition)
                        assertEquals(first.messages.first().sentAt, first.firstPosition!!.sentAt)
                        assertEquals(first.messages.last().sentAt, first.lastPosition!!.sentAt)
                        val content = first.messages.first().content as SDKMessageContent.Standard
                        assertEquals("first", (content.value as MessageContent.Text).v1)
                        assertTrue(first.lastPosition!!.messageCursor.isNotEmpty())
                        val next =
                            conversation.messageRecoveryPage(
                                ListMessagesOptions(limit = 2u, kind = MessageKind.APPLICATION),
                                after = first.lastPosition,
                            )
                        assertEquals(ids.drop(2), next.messages.map { it.id })
                        assertFalse(next.hasMore)
                        val older =
                            conversation.messageRecoveryPage(
                                ListMessagesOptions(
                                    limit = 2u,
                                    kind = MessageKind.APPLICATION,
                                    direction = MessageOrder.DESCENDING,
                                ),
                                before = next.firstPosition,
                            )
                        assertEquals(ids.take(2).reversed(), older.messages.map { it.id })
                        conversation.publishMessage(ids[1])
                        val fresh = conversation.sendText("after publication", SendOptions(optimistic = true))
                        val resumed = conversation.messageRecoveryPage(after = first.lastPosition)
                        assertEquals(listOf(fresh), resumed.messages.map { it.id })
                        assertEquals(
                            listOf(fresh),
                            conversation
                                .messageRecoveryPage()
                                .messages
                                .filter { it.id == fresh }
                                .map { it.id },
                        )
                    }
                }
            }
        }
}
