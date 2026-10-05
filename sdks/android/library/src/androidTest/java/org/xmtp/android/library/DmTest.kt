package org.xmtp.android.library

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import uniffi.xmtp_sdk.*

class DmTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private val alix get() = fixtures.alixClient
    private val bo get() = fixtures.boClient

    @Before override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
    }

    private fun text(message: Message): String? =
        ((message.content as? SDKMessageContent.Standard)?.value as? MessageContent.Text)?.v1

    private suspend fun find(
        client: SDKClient,
        peer: InboxId,
    ): Dm = checkNotNull(client.conversations().getDmByInboxId(peer))

    @Test fun testCannotCreateDmWithMemberNotOnV3() =
        runBlocking {
            val unregistered = createWallet().identity()
            try {
                bo.conversations().createDm(unregistered)
                fail("An unregistered identity must fail")
            } catch (_: XmtpException) {
                assertNull(bo.conversations().getDmByIdentity(unregistered))
            }
        }

    @Test fun testCannotStartDmWithSelf() =
        runBlocking {
            try {
                bo.conversations().createDm(bo.inboxId())
                fail("Recipient is sender")
            } catch (_: XmtpException) {
                assertTrue(bo.conversations().listDms(null).isEmpty())
            }
        }

    @Test fun testCanSendMessageToDm() =
        runBlocking {
            val dm = bo.conversations().createDm(alix.inboxId())
            dm.sendText("howdy")
            val id = dm.sendText("gm")
            dm.sync()
            assertEquals("gm", text(dm.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first()))
            assertEquals(id, dm.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first().id)
            assertEquals(
                DeliveryStatus.PUBLISHED,
                dm.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first().deliveryStatus,
            )
            assertEquals(3, dm.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).size)
            alix.conversations().sync()
            val peer = find(alix, bo.inboxId())
            peer.sync()
            assertEquals(3, peer.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).size)
            assertEquals("gm", text(peer.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first()))
        }

    @Test fun testCanStreamDmMessages() =
        runBlocking {
            val dm = bo.conversations().createDm(alix.inboxId())
            alix.conversations().sync()
            val peer = checkNotNull(alix.conversations().getDmByIdentity(fixtures.bo))
            dm.sync()
            val retained = dm.messageHistorySnapshot(10u).messages
            assertEquals(1, retained.size)
            assertEquals(MessageKind.MEMBERSHIP_CHANGE, retained.single().kind)
            val messages = StreamTestMessages()
            val job = launch(Dispatchers.IO) { bo.messages(dm).collect { messages.add(it) } }
            try {
                messages.awaitHistory(retained)
                val first = peer.sendText("hi")
                messages.awaitApplications(listOf(first to "hi"))
                val second = peer.sendText("hi again")
                messages.awaitApplications(listOf(first to "hi", second to "hi again"))
                messages.awaitHistory(dm.messageHistorySnapshot(10u).messages)
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }
}
