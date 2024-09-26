package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.MessageDeliveryStatus
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress
import java.security.SecureRandom

@RunWith(AndroidJUnit4::class)
class V3ClientTest {
    private lateinit var alixV2Wallet: PrivateKeyBuilder
    private lateinit var boV3Wallet: PrivateKeyBuilder
    private lateinit var alixV2: PrivateKey
    private lateinit var alixV2Client: Client
    private lateinit var boV3: PrivateKey
    private lateinit var boV3Client: Client
    private lateinit var caroV2V3Wallet: PrivateKeyBuilder
    private lateinit var caroV2V3: PrivateKey
    private lateinit var caroV2V3Client: Client

    @Before
    fun setUp() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext

        // Pure V2
        alixV2Wallet = PrivateKeyBuilder()
        alixV2 = alixV2Wallet.getPrivateKey()
        alixV2Client = runBlocking {
            Client().create(
                account = alixV2Wallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, isSecure = false)
                )
            )
        }

        // Pure V3
        boV3Wallet = PrivateKeyBuilder()
        boV3 = boV3Wallet.getPrivateKey()
        boV3Client = runBlocking {
            Client().createOrBuild(
                account = boV3Wallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    enableV3 = true,
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }

        // Both V3 & V2
        caroV2V3Wallet = PrivateKeyBuilder()
        caroV2V3 = caroV2V3Wallet.getPrivateKey()
        caroV2V3Client =
            runBlocking {
                Client().create(
                    account = caroV2V3Wallet,
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
    fun testsCanCreateGroup() {
        val group = runBlocking { boV3Client.conversations.newGroup(listOf(caroV2V3.walletAddress)) }
        assertEquals(
            runBlocking { group.members().map { it.inboxId }.sorted() },
            listOf(caroV2V3Client.inboxId, boV3Client.inboxId).sorted()
        )

        Assert.assertThrows("Recipient not on network", XMTPException::class.java) {
            runBlocking { boV3Client.conversations.newGroup(listOf(alixV2.walletAddress)) }
        }
    }

    @Test
    fun testsCanSendMessages() {
        val group = runBlocking { boV3Client.conversations.newGroup(listOf(caroV2V3.walletAddress)) }
        runBlocking { group.send("howdy") }
        val messageId = runBlocking { group.send("gm") }
        runBlocking { group.sync() }
        assertEquals(group.messages().first().body, "gm")
        assertEquals(group.messages().first().id, messageId)
        assertEquals(group.messages().first().deliveryStatus, MessageDeliveryStatus.PUBLISHED)
        assertEquals(group.messages().size, 3)

        runBlocking { caroV2V3Client.conversations.syncGroups() }
        val sameGroup = runBlocking { caroV2V3Client.conversations.listGroups().last() }
        runBlocking { sameGroup.sync() }
        assertEquals(sameGroup.messages().size, 2)
        assertEquals(sameGroup.messages().first().body, "gm")
    }

    @Test
    fun testGroupConsent() {
        runBlocking {
            val group = boV3Client.conversations.newGroup(listOf(caroV2V3.walletAddress))
            assert(boV3Client.contacts.isGroupAllowed(group.id))
            assertEquals(group.consentState(), ConsentState.ALLOWED)

            boV3Client.contacts.denyGroups(listOf(group.id))
            assert(boV3Client.contacts.isGroupDenied(group.id))
            assertEquals(group.consentState(), ConsentState.DENIED)

            group.updateConsentState(ConsentState.ALLOWED)
            assert(boV3Client.contacts.isGroupAllowed(group.id))
            assertEquals(group.consentState(), ConsentState.ALLOWED)
        }
    }

    @Test
    fun testCanAllowAndDenyInboxId() {
        runBlocking {
            val boGroup = boV3Client.conversations.newGroup(listOf(caroV2V3.walletAddress))
            assert(!boV3Client.contacts.isInboxAllowed(caroV2V3Client.inboxId))
            assert(!boV3Client.contacts.isInboxDenied(caroV2V3Client.inboxId))

            boV3Client.contacts.allowInboxes(listOf(caroV2V3Client.inboxId))
            var caroMember = boGroup.members().firstOrNull { it.inboxId == caroV2V3Client.inboxId }
            assertEquals(caroMember!!.consentState, ConsentState.ALLOWED)

            assert(boV3Client.contacts.isInboxAllowed(caroV2V3Client.inboxId))
            assert(!boV3Client.contacts.isInboxDenied(caroV2V3Client.inboxId))
            assert(boV3Client.contacts.isAllowed(caroV2V3Client.address))
            assert(!boV3Client.contacts.isDenied(caroV2V3Client.address))

            boV3Client.contacts.denyInboxes(listOf(caroV2V3Client.inboxId))
            caroMember = boGroup.members().firstOrNull { it.inboxId == caroV2V3Client.inboxId }
            assertEquals(caroMember!!.consentState, ConsentState.DENIED)

            assert(!boV3Client.contacts.isInboxAllowed(caroV2V3Client.inboxId))
            assert(boV3Client.contacts.isInboxDenied(caroV2V3Client.inboxId))

            // Cannot check inboxId for alix because they do not have an inboxID as V2 only client.
            boV3Client.contacts.allow(listOf(alixV2Client.address))
            assert(boV3Client.contacts.isAllowed(alixV2Client.address))
            assert(!boV3Client.contacts.isDenied(alixV2Client.address))
        }
    }

    @Test
    fun testCanStreamAllMessagesFromV2andV3Users() {
        val group = runBlocking { boV3Client.conversations.newGroup(listOf(caroV2V3.walletAddress)) }
        val conversation =
            runBlocking { alixV2Client.conversations.newConversation(caroV2V3.walletAddress) }
        runBlocking { caroV2V3Client.conversations.syncGroups() }

        val allMessages = mutableListOf<DecodedMessage>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                caroV2V3Client.conversations.streamAllMessages(includeGroups = true)
                    .collect { message ->
                        allMessages.add(message)
                    }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(1000)
        runBlocking {
            group.send("hi")
            conversation.send("hi")
        }
        Thread.sleep(1000)
        assertEquals(2, allMessages.size)
        job.cancel()
    }

    @Test
    fun testCanStreamGroupsAndConversationsFromV2andV3Users() {
        val allMessages = mutableListOf<String>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                caroV2V3Client.conversations.streamAll()
                    .collect { message ->
                        allMessages.add(message.topic)
                    }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(1000)

        runBlocking {
            alixV2Client.conversations.newConversation(caroV2V3.walletAddress)
            Thread.sleep(1000)
            boV3Client.conversations.newGroup(listOf(caroV2V3.walletAddress))
        }

        Thread.sleep(2000)
        assertEquals(2, allMessages.size)
        job.cancel()
    }
}
