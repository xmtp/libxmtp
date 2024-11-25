package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress
import java.security.SecureRandom

@RunWith(AndroidJUnit4::class)
class ConversationsTest {
    private lateinit var alixWallet: PrivateKeyBuilder
    private lateinit var boWallet: PrivateKeyBuilder
    private lateinit var alix: PrivateKey
    private lateinit var alixClient: Client
    private lateinit var bo: PrivateKey
    private lateinit var boClient: Client
    private lateinit var caroWallet: PrivateKeyBuilder
    private lateinit var caro: PrivateKey
    private lateinit var caroClient: Client
    private lateinit var fixtures: Fixtures

    @Before
    fun setUp() {
        fixtures = fixtures()
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
    fun testsCanFindConversationByTopic() {
        val group =
            runBlocking { boClient.conversations.newGroup(listOf(caro.walletAddress)) }
        val dm = runBlocking { boClient.conversations.findOrCreateDm(caro.walletAddress) }

        val sameDm = boClient.findConversationByTopic(dm.topic)
        val sameGroup = boClient.findConversationByTopic(group.topic)
        assertEquals(group.id, sameGroup?.id)
        assertEquals(dm.id, sameDm?.id)
    }

    @Test
    fun testsCanListConversations() {
        runBlocking { boClient.conversations.findOrCreateDm(caro.walletAddress) }
        runBlocking { boClient.conversations.newGroup(listOf(caro.walletAddress)) }
        assertEquals(runBlocking { boClient.conversations.list().size }, 2)
        assertEquals(runBlocking { boClient.conversations.listDms().size }, 1)
        assertEquals(runBlocking { boClient.conversations.listGroups().size }, 1)

        runBlocking { caroClient.conversations.sync() }
        assertEquals(
            runBlocking { caroClient.conversations.list().size },
            2
        )
        assertEquals(runBlocking { caroClient.conversations.listGroups().size }, 1)
    }

    @Test
    fun testsCanListConversationsFiltered() {
        runBlocking { boClient.conversations.findOrCreateDm(caro.walletAddress) }
        val group =
            runBlocking { boClient.conversations.newGroup(listOf(caro.walletAddress)) }
        assertEquals(runBlocking { boClient.conversations.list().size }, 2)
        assertEquals(
            runBlocking { boClient.conversations.list(consentState = ConsentState.ALLOWED).size },
            2
        )
        runBlocking { group.updateConsentState(ConsentState.DENIED) }
        assertEquals(
            runBlocking { boClient.conversations.list(consentState = ConsentState.ALLOWED).size },
            1
        )
        assertEquals(
            runBlocking { boClient.conversations.list(consentState = ConsentState.DENIED).size },
            1
        )
        assertEquals(runBlocking { boClient.conversations.list().size }, 2)
    }

    @Test
    fun testCanListConversationsOrder() {
        val dm = runBlocking { boClient.conversations.findOrCreateDm(caro.walletAddress) }
        val group1 =
            runBlocking { boClient.conversations.newGroup(listOf(caro.walletAddress)) }
        val group2 =
            runBlocking { boClient.conversations.newGroup(listOf(caro.walletAddress)) }
        runBlocking { dm.send("Howdy") }
        runBlocking { group2.send("Howdy") }
        runBlocking { boClient.conversations.syncAllConversations() }
        val conversations = runBlocking { boClient.conversations.list() }
        val conversationsOrdered =
            runBlocking { boClient.conversations.list(order = Conversations.ConversationOrder.LAST_MESSAGE) }
        assertEquals(conversations.size, 3)
        assertEquals(conversationsOrdered.size, 3)
        assertEquals(conversations.map { it.id }, listOf(dm.id, group1.id, group2.id))
        assertEquals(conversationsOrdered.map { it.id }, listOf(group2.id, dm.id, group1.id))
    }

    @Test
    fun testCanStreamAllMessages() {
        val group =
            runBlocking { caroClient.conversations.newGroup(listOf(bo.walletAddress)) }
        val conversation =
            runBlocking { boClient.conversations.findOrCreateDm(caro.walletAddress) }
        runBlocking { boClient.conversations.sync() }

        val allMessages = mutableListOf<DecodedMessage>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                boClient.conversations.streamAllMessages()
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
    fun testCanStreamGroupsAndConversations() {
        val allMessages = mutableListOf<String>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                boClient.conversations.stream()
                    .collect { message ->
                        allMessages.add(message.topic)
                    }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(1000)

        runBlocking {
            caroClient.conversations.newGroup(listOf(bo.walletAddress))
            Thread.sleep(1000)
            boClient.conversations.findOrCreateDm(caro.walletAddress)
        }

        Thread.sleep(2000)
        assertEquals(2, allMessages.size)
        job.cancel()
    }

    @Test
    fun testSyncConsent() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixWallet = PrivateKeyBuilder()

        val alixClient = runBlocking {
            Client().create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val dm = runBlocking { alixClient.conversations.findOrCreateDm(bo.walletAddress) }
        runBlocking {
            dm.updateConsentState(ConsentState.DENIED)
            assertEquals(dm.consentState(), ConsentState.DENIED)
            boClient.conversations.sync()
        }
        val boDm = runBlocking { boClient.findConversation(dm.id) }

        val alixClient2 = runBlocking {
            Client().create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key,
                    dbDirectory = context.filesDir.absolutePath.toString()
                )
            )
        }

        val state = runBlocking { alixClient2.inboxState(true) }
        assertEquals(state.installations.size, 2)

        runBlocking {
            boClient.conversations.sync()
            boDm?.sync()
            alixClient.conversations.sync()
            alixClient2.conversations.sync()
            alixClient2.preferences.syncConsent()
            alixClient.conversations.syncAllConversations()
            Thread.sleep(2000)
            alixClient2.conversations.syncAllConversations()
            Thread.sleep(2000)
            val dm2 = alixClient2.findConversation(dm.id)!!
            assertEquals(ConsentState.DENIED, dm2.consentState())
            alixClient2.preferences.setConsentState(
                listOf(
                    ConsentListEntry(
                        dm2.id,
                        EntryType.CONVERSATION_ID,
                        ConsentState.ALLOWED
                    )
                )
            )
            assertEquals(
                alixClient2.preferences.conversationState(dm2.id),
                ConsentState.ALLOWED
            )
            assertEquals(dm2.consentState(), ConsentState.ALLOWED)
        }
    }

    @Test
    fun testStreamConsent() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixWallet = PrivateKeyBuilder()

        val alixClient = runBlocking {
            Client().create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val alixGroup = runBlocking { alixClient.conversations.newGroup(listOf(bo.walletAddress)) }

        val alixClient2 = runBlocking {
            Client().create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key,
                    dbDirectory = context.filesDir.absolutePath.toString()
                )
            )
        }

        runBlocking {
            alixGroup.send("Hello")
            alixClient2.conversations.sync()
            alixClient.conversations.syncAllConversations()
            alixClient2.conversations.syncAllConversations()
        }
        val alix2Group = alixClient2.findGroup(alixGroup.id)!!
        val consent = mutableListOf<ConsentListEntry>()
        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.preferences.streamConsent()
                    .collect { entry ->
                        consent.add(entry)
                    }
            } catch (e: Exception) {
            }
        }

        Thread.sleep(1000)

        runBlocking {
            alix2Group.updateConsentState(ConsentState.DENIED)
            val dm3 = alixClient2.conversations.newConversation(caro.walletAddress)
            dm3.updateConsentState(ConsentState.DENIED)
            alixClient.conversations.sync()
            alixClient2.conversations.sync()
            alixClient.conversations.syncAllConversations()
            alixClient2.conversations.syncAllConversations()
        }

        Thread.sleep(2000)
        assertEquals(3, consent.size)
        assertEquals(alixGroup.consentState(), ConsentState.DENIED)
        job.cancel()
    }
}
