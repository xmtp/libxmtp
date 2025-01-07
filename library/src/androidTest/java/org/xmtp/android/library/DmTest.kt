package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import app.cash.turbine.test
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.Conversations.ConversationType
import org.xmtp.android.library.codecs.ContentTypeReaction
import org.xmtp.android.library.codecs.Reaction
import org.xmtp.android.library.codecs.ReactionAction
import org.xmtp.android.library.codecs.ReactionCodec
import org.xmtp.android.library.codecs.ReactionSchema
import org.xmtp.android.library.libxmtp.Message
import org.xmtp.android.library.libxmtp.Message.MessageDeliveryStatus
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress

@RunWith(AndroidJUnit4::class)
class DmTest {
    private lateinit var alixWallet: PrivateKeyBuilder
    private lateinit var boWallet: PrivateKeyBuilder
    private lateinit var caroWallet: PrivateKeyBuilder
    private lateinit var alix: PrivateKey
    private lateinit var alixClient: Client
    private lateinit var bo: PrivateKey
    private lateinit var boClient: Client
    private lateinit var caro: PrivateKey
    private lateinit var caroClient: Client

    @Before
    fun setUp() {
        val fixtures = fixtures()
        alixWallet = fixtures.alixAccount
        alix = fixtures.alix
        boWallet = fixtures.boAccount
        bo = fixtures.bo
        caroWallet = fixtures.caroAccount
        caro = fixtures.caro

        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
        caroClient = fixtures.caroClient
    }

    @Test
    fun testCanCreateADm() {
        runBlocking {
            val convo1 = boClient.conversations.findOrCreateDm(alix.walletAddress)
            alixClient.conversations.sync()
            val sameConvo1 = alixClient.conversations.findOrCreateDm(bo.walletAddress)
            assertEquals(convo1.id, sameConvo1.id)
        }
    }

    @Test
    fun testsCanFindDmByInboxId() {
        runBlocking {
            val dm = boClient.conversations.findOrCreateDm(caro.walletAddress)

            val caroDm = boClient.findDmByInboxId(caroClient.inboxId)
            val alixDm = boClient.findDmByInboxId(alixClient.inboxId)
            assertNull(alixDm)
            assertEquals(caroDm?.id, dm.id)
        }
    }

    @Test
    fun testsCanFindDmByAddress() {
        runBlocking {
            val dm = boClient.conversations.findOrCreateDm(caro.walletAddress)

            val caroDm = boClient.findDmByAddress(caro.walletAddress)
            val alixDm = boClient.findDmByAddress(alix.walletAddress)
            assertNull(alixDm)
            assertEquals(caroDm?.id, dm.id)
        }
    }

    @Test
    fun testCanListDmMembers() {
        val dm = runBlocking {
            boClient.conversations.findOrCreateDm(
                alix.walletAddress,
            )
        }
        assertEquals(
            runBlocking { dm.members().map { it.inboxId }.sorted() },
            listOf(
                alixClient.inboxId,
                boClient.inboxId
            ).sorted()
        )

        assertEquals(
            runBlocking {
                Conversation.Dm(dm).members().map { it.inboxId }.sorted()
            },
            listOf(
                alixClient.inboxId,
                boClient.inboxId
            ).sorted()
        )

        assertEquals(
            runBlocking
            { dm.peerInboxId },
            alixClient.inboxId,
        )
    }

    @Test
    fun testCannotCreateDmWithMemberNotOnV3() {
        val chuxAccount = PrivateKeyBuilder()
        val chux: PrivateKey = chuxAccount.getPrivateKey()

        assertThrows("Recipient not on network", XMTPException::class.java) {
            runBlocking { boClient.conversations.findOrCreateDm(chux.walletAddress) }
        }
    }

    @Test
    fun testCannotStartDmWithSelf() {
        assertThrows("Recipient is sender", XMTPException::class.java) {
            runBlocking { boClient.conversations.findOrCreateDm(bo.walletAddress) }
        }
    }

    @Test
    fun testDmStartsWithAllowedState() {
        runBlocking {
            val dm = boClient.conversations.findOrCreateDm(alix.walletAddress)
            dm.send("howdy")
            dm.send("gm")
            dm.sync()
            assertEquals(
                boClient.preferences.conversationState(dm.id),
                ConsentState.ALLOWED
            )
            assertEquals(dm.consentState(), ConsentState.ALLOWED)
        }
    }

    @Test
    fun testsCanListDmsFiltered() {
        runBlocking { boClient.conversations.findOrCreateDm(caro.walletAddress) }
        runBlocking { boClient.conversations.newGroup(listOf(caro.walletAddress)) }
        val dm = runBlocking { boClient.conversations.findOrCreateDm(alix.walletAddress) }
        assertEquals(runBlocking { boClient.conversations.listDms().size }, 2)
        assertEquals(
            runBlocking { boClient.conversations.listDms(consentState = ConsentState.ALLOWED).size },
            2
        )
        runBlocking { dm.updateConsentState(ConsentState.DENIED) }
        assertEquals(
            runBlocking { boClient.conversations.listDms(consentState = ConsentState.ALLOWED).size },
            1
        )
        assertEquals(
            runBlocking { boClient.conversations.listDms(consentState = ConsentState.DENIED).size },
            1
        )
        assertEquals(runBlocking { boClient.conversations.listDms().size }, 2)
    }

    @Test
    fun testCanListDmsOrder() {
        val dm1 = runBlocking { boClient.conversations.findOrCreateDm(caro.walletAddress) }
        val dm2 =
            runBlocking { boClient.conversations.findOrCreateDm(alix.walletAddress) }
        val group =
            runBlocking { boClient.conversations.newGroup(listOf(caro.walletAddress)) }
        runBlocking { dm2.send("Howdy") }
        runBlocking { group.send("Howdy") }
        runBlocking { boClient.conversations.syncAllConversations() }
        val conversations = runBlocking { boClient.conversations.listDms() }
        assertEquals(conversations.size, 2)
        assertEquals(conversations.map { it.id }, listOf(dm2.id, dm1.id))
    }

    @Test
    fun testCanSendMessageToDm() {
        val dm = runBlocking { boClient.conversations.findOrCreateDm(alix.walletAddress) }
        runBlocking { dm.send("howdy") }
        val messageId = runBlocking { dm.send("gm") }
        runBlocking { dm.sync() }
        assertEquals(runBlocking { dm.messages() }.first().body, "gm")
        assertEquals(runBlocking { dm.messages() }.first().id, messageId)
        assertEquals(
            runBlocking { dm.messages() }.first().deliveryStatus,
            MessageDeliveryStatus.PUBLISHED
        )
        assertEquals(runBlocking { dm.messages() }.size, 2)

        runBlocking { alixClient.conversations.sync() }
        val sameDm = runBlocking { alixClient.conversations.listDms().last() }
        runBlocking { sameDm.sync() }
        assertEquals(runBlocking { sameDm.messages() }.size, 2)
        assertEquals(runBlocking { sameDm.messages() }.first().body, "gm")
    }

    @Test
    fun testCanListDmMessages() {
        val dm = runBlocking { boClient.conversations.findOrCreateDm(alix.walletAddress) }
        runBlocking {
            dm.send("howdy")
            dm.send("gm")
        }

        assertEquals(runBlocking { dm.messages() }.size, 2)
        assertEquals(
            runBlocking { dm.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED) }.size,
            2
        )
        runBlocking { dm.sync() }
        assertEquals(runBlocking { dm.messages() }.size, 2)
        assertEquals(
            runBlocking { dm.messages(deliveryStatus = MessageDeliveryStatus.UNPUBLISHED) }.size,
            0
        )
        assertEquals(
            runBlocking { dm.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED) }.size,
            2
        )

        runBlocking { alixClient.conversations.sync() }
        val sameDm = runBlocking { alixClient.conversations.listDms().last() }
        runBlocking { sameDm.sync() }
        assertEquals(
            runBlocking { sameDm.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED) }.size,
            2
        )
    }

    @Test
    fun testCanSendContentTypesToDm() {
        Client.register(codec = ReactionCodec())

        val dm = runBlocking { boClient.conversations.findOrCreateDm(alix.walletAddress) }
        runBlocking { dm.send("gm") }
        runBlocking { dm.sync() }
        val messageToReact = runBlocking { dm.messages() }[0]

        val reaction = Reaction(
            reference = messageToReact.id,
            action = ReactionAction.Added,
            content = "U+1F603",
            schema = ReactionSchema.Unicode
        )

        runBlocking {
            dm.send(
                content = reaction,
                options = SendOptions(contentType = ContentTypeReaction)
            )
        }
        runBlocking { dm.sync() }

        val messages = runBlocking { dm.messages() }
        assertEquals(messages.size, 2)
        val content: Reaction? = messages.first().content()
        assertEquals("U+1F603", content?.content)
        assertEquals(messageToReact.id, content?.reference)
        assertEquals(ReactionAction.Added, content?.action)
        assertEquals(ReactionSchema.Unicode, content?.schema)
    }

    @Test
    fun testCanStreamDmMessages() = kotlinx.coroutines.test.runTest {
        val group = boClient.conversations.findOrCreateDm(alix.walletAddress.lowercase())
        alixClient.conversations.sync()
        val alixDm = alixClient.findDmByAddress(bo.walletAddress)
        group.streamMessages().test {
            alixDm?.send("hi")
            assertEquals("hi", awaitItem().body)
            alixDm?.send("hi again")
            assertEquals("hi again", awaitItem().body)
        }
    }

    @Test
    fun testCanStreamAllMessages() {
        val boDm = runBlocking { boClient.conversations.findOrCreateDm(alix.walletAddress) }
        runBlocking { alixClient.conversations.sync() }

        val allMessages = mutableListOf<Message>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.conversations.streamAllMessages(type = ConversationType.DMS)
                    .collect { message ->
                        allMessages.add(message)
                    }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(2500)

        for (i in 0 until 2) {
            runBlocking { boDm.send(text = "Message $i") }
            Thread.sleep(100)
        }
        assertEquals(2, allMessages.size)

        val caroDm =
            runBlocking { caroClient.conversations.findOrCreateDm(alixClient.address) }
        Thread.sleep(2500)

        for (i in 0 until 2) {
            runBlocking { caroDm.send(text = "Message $i") }
            Thread.sleep(100)
        }

        assertEquals(4, allMessages.size)

        job.cancel()
    }

    @Test
    fun testCanStreamConversations() = kotlinx.coroutines.test.runTest {
        boClient.conversations.stream(type = ConversationType.DMS).test {
            val dm =
                alixClient.conversations.findOrCreateDm(bo.walletAddress)
            assertEquals(dm.id, awaitItem().id)
            val dm2 =
                caroClient.conversations.findOrCreateDm(bo.walletAddress)
            assertEquals(dm2.id, awaitItem().id)
        }
    }

    @Test
    fun testDmConsent() {
        runBlocking {
            val dm =
                boClient.conversations.findOrCreateDm(alix.walletAddress)
            assertEquals(
                boClient.preferences.conversationState(dm.id),
                ConsentState.ALLOWED
            )

            assertEquals(dm.consentState(), ConsentState.ALLOWED)

            boClient.preferences.setConsentState(
                listOf(
                    ConsentRecord(
                        dm.id,
                        EntryType.CONVERSATION_ID,
                        ConsentState.DENIED
                    )
                )
            )
            assertEquals(
                boClient.preferences.conversationState(dm.id),
                ConsentState.DENIED
            )
            assertEquals(dm.consentState(), ConsentState.DENIED)

            boClient.preferences.setConsentState(
                listOf(
                    ConsentRecord(
                        dm.id,
                        EntryType.CONVERSATION_ID,
                        ConsentState.ALLOWED
                    )
                )
            )
            assertEquals(
                boClient.preferences.conversationState(dm.id),
                ConsentState.ALLOWED
            )
            assertEquals(dm.consentState(), ConsentState.ALLOWED)
        }
    }
}
