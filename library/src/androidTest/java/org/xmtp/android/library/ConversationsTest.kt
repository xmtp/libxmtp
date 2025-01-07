package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
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

        val sameDm = runBlocking { boClient.findConversationByTopic(dm.topic) }
        val sameGroup = runBlocking { boClient.findConversationByTopic(group.topic) }
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
        val dmMessage = runBlocking { dm.send("Howdy") }
        val groupMessage = runBlocking { group2.send("Howdy") }
        runBlocking { boClient.conversations.syncAllConversations() }
        val conversations = runBlocking { boClient.conversations.list() }
        assertEquals(conversations.size, 3)
        assertEquals(conversations.map { it.id }, listOf(group2.id, dm.id, group1.id))
        runBlocking {
            assertEquals(group2.lastMessage()!!.id, groupMessage)
            assertEquals(dm.lastMessage()!!.id, dmMessage)
        }
    }

    @Test
    fun testsCanSyncAllConversationsFiltered() {
        runBlocking { boClient.conversations.findOrCreateDm(caro.walletAddress) }
        val group =
            runBlocking { boClient.conversations.newGroup(listOf(caro.walletAddress)) }
        assert(runBlocking { boClient.conversations.syncAllConversations() }.toInt() >= 2)
        assert(
            runBlocking { boClient.conversations.syncAllConversations(consentState = ConsentState.ALLOWED) }.toInt() >= 2
        )
        assert(
            runBlocking { boClient.conversations.syncAllConversations(consentState = ConsentState.DENIED) }.toInt() <= 1
        )
        runBlocking { group.updateConsentState(ConsentState.DENIED) }
        assert(
            runBlocking { boClient.conversations.syncAllConversations(consentState = ConsentState.ALLOWED) }.toInt() <= 2
        )
        assert(
            runBlocking { boClient.conversations.syncAllConversations(consentState = ConsentState.DENIED) }.toInt() <= 2
        )
        assert(runBlocking { boClient.conversations.syncAllConversations() }.toInt() >= 2)
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
}
