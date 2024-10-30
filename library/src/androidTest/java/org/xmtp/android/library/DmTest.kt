package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.cash.turbine.test
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentTypeReaction
import org.xmtp.android.library.codecs.Reaction
import org.xmtp.android.library.codecs.ReactionAction
import org.xmtp.android.library.codecs.ReactionCodec
import org.xmtp.android.library.codecs.ReactionSchema
import org.xmtp.android.library.messages.DecryptedMessage
import org.xmtp.android.library.messages.MessageDeliveryStatus
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress
import java.security.SecureRandom

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
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        alixWallet = PrivateKeyBuilder()
        alix = alixWallet.getPrivateKey()
        alixClient = runBlocking {
            Client().createV3(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    enableV3 = true,
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        boWallet = PrivateKeyBuilder()
        bo = boWallet.getPrivateKey()
        boClient = runBlocking {
            Client().createV3(
                account = boWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    enableV3 = true,
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }

        caroWallet = PrivateKeyBuilder()
        caro = caroWallet.getPrivateKey()
        caroClient = runBlocking {
            Client().createV3(
                account = caroWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    enableV3 = true,
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
    }

    @Test
    fun testCanCreateADm() {
        runBlocking {
            val convo1 = boClient.conversations.findOrCreateDm(alix.walletAddress)
            alixClient.conversations.syncConversations()
            val sameConvo1 = alixClient.conversations.findOrCreateDm(bo.walletAddress)
            assertEquals(convo1.id, sameConvo1.id)
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
        runBlocking { Client().create(account = chuxAccount) }

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
            assert(boClient.contacts.isGroupAllowed(dm.id))
            assertEquals(boClient.contacts.consentList.groupState(dm.id), ConsentState.ALLOWED)
            assertEquals(dm.consentState(), ConsentState.ALLOWED)
        }
    }

    @Test
    fun testCanSendMessageToDm() {
        val dm = runBlocking { boClient.conversations.findOrCreateDm(alix.walletAddress) }
        runBlocking { dm.send("howdy") }
        val messageId = runBlocking { dm.send("gm") }
        runBlocking { dm.sync() }
        assertEquals(dm.messages().first().body, "gm")
        assertEquals(dm.messages().first().id, messageId)
        assertEquals(dm.messages().first().deliveryStatus, MessageDeliveryStatus.PUBLISHED)
        assertEquals(dm.messages().size, 3)

        runBlocking { alixClient.conversations.syncConversations() }
        val sameDm = runBlocking { alixClient.conversations.listDms().last() }
        runBlocking { sameDm.sync() }
        assertEquals(sameDm.messages().size, 2)
        assertEquals(sameDm.messages().first().body, "gm")
    }

    @Test
    fun testCanListDmMessages() {
        val dm = runBlocking { boClient.conversations.findOrCreateDm(alix.walletAddress) }
        runBlocking {
            dm.send("howdy")
            dm.send("gm")
        }

        assertEquals(dm.messages().size, 3)
        assertEquals(dm.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED).size, 3)
        runBlocking { dm.sync() }
        assertEquals(dm.messages().size, 3)
        assertEquals(dm.messages(deliveryStatus = MessageDeliveryStatus.UNPUBLISHED).size, 0)
        assertEquals(dm.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED).size, 3)

        runBlocking { alixClient.conversations.syncConversations() }
        val sameDm = runBlocking { alixClient.conversations.listDms().last() }
        runBlocking { sameDm.sync() }
        assertEquals(sameDm.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED).size, 2)
    }

    @Test
    fun testCanSendContentTypesToDm() {
        Client.register(codec = ReactionCodec())

        val dm = runBlocking { boClient.conversations.findOrCreateDm(alix.walletAddress) }
        runBlocking { dm.send("gm") }
        runBlocking { dm.sync() }
        val messageToReact = dm.messages()[0]

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

        val messages = dm.messages()
        assertEquals(messages.size, 3)
        val content: Reaction? = messages.first().content()
        assertEquals("U+1F603", content?.content)
        assertEquals(messageToReact.id, content?.reference)
        assertEquals(ReactionAction.Added, content?.action)
        assertEquals(ReactionSchema.Unicode, content?.schema)
    }

    @Test
    fun testCanStreamDmMessages() = kotlinx.coroutines.test.runTest {
        val group = boClient.conversations.findOrCreateDm(alix.walletAddress.lowercase())
        alixClient.conversations.syncConversations()
        val alixDm = alixClient.findDm(bo.walletAddress)
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
        runBlocking { alixClient.conversations.syncConversations() }

        val allMessages = mutableListOf<DecodedMessage>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.conversations.streamAllConversationMessages().collect { message ->
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
    fun testCanStreamDecryptedDmMessages() = kotlinx.coroutines.test.runTest {
        val dm = boClient.conversations.findOrCreateDm(alix.walletAddress)
        alixClient.conversations.syncConversations()
        val alixDm = alixClient.findDm(bo.walletAddress)
        dm.streamDecryptedMessages().test {
            alixDm?.send("hi")
            assertEquals("hi", awaitItem().encodedContent.content.toStringUtf8())
            alixDm?.send("hi again")
            assertEquals("hi again", awaitItem().encodedContent.content.toStringUtf8())
        }
    }

    @Test
    fun testCanStreamAllDecryptedDmMessages() {
        val dm = runBlocking { boClient.conversations.findOrCreateDm(alix.walletAddress) }
        runBlocking { alixClient.conversations.syncConversations() }

        val allMessages = mutableListOf<DecryptedMessage>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.conversations.streamAllConversationDecryptedMessages().collect { message ->
                    allMessages.add(message)
                }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(2500)

        for (i in 0 until 2) {
            runBlocking { dm.send(text = "Message $i") }
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
        boClient.conversations.streamConversations().test {
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
            assert(boClient.contacts.isGroupAllowed(dm.id))
            assertEquals(dm.consentState(), ConsentState.ALLOWED)

            boClient.contacts.denyGroups(listOf(dm.id))
            assert(boClient.contacts.isGroupDenied(dm.id))
            assertEquals(dm.consentState(), ConsentState.DENIED)

            dm.updateConsentState(ConsentState.ALLOWED)
            assert(boClient.contacts.isGroupAllowed(dm.id))
            assertEquals(dm.consentState(), ConsentState.ALLOWED)
        }
    }
}
