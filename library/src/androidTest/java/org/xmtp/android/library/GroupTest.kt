package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.cash.turbine.test
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
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress

@RunWith(AndroidJUnit4::class)
class GroupTest {
    lateinit var fakeApiClient: FakeApiClient
    lateinit var alixWallet: PrivateKeyBuilder
    lateinit var boWallet: PrivateKeyBuilder
    lateinit var alix: PrivateKey
    lateinit var alixClient: Client
    lateinit var bo: PrivateKey
    lateinit var boClient: Client
    lateinit var caroWallet: PrivateKeyBuilder
    lateinit var caro: PrivateKey
    lateinit var caroClient: Client
    lateinit var fixtures: Fixtures

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        fixtures =
            fixtures(
                clientOptions = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    enableAlphaMls = true,
                    appContext = context
                )
            )
        alixWallet = fixtures.aliceAccount
        alix = fixtures.alice
        boWallet = fixtures.bobAccount
        bo = fixtures.bob
        caroWallet = fixtures.caroAccount
        caro = fixtures.caro

        fakeApiClient = fixtures.fakeApiClient
        alixClient = fixtures.aliceClient
        boClient = fixtures.bobClient
        caroClient = fixtures.caroClient
    }

    @Test
    fun testCanCreateAGroup() {
        val group = boClient.conversations.newGroup(listOf(alix.walletAddress))
        assert(group.id.isNotEmpty())
    }

    @Test
    fun testCanListGroupMembers() {
        val group = boClient.conversations.newGroup(
            listOf(
                alix.walletAddress,
                caro.walletAddress
            )
        )
        assertEquals(
            group.memberAddresses().sorted(),
            listOf(
                caro.walletAddress.lowercase(),
                alix.walletAddress.lowercase(),
                bo.walletAddress.lowercase()
            ).sorted()
        )
    }

    @Test
    fun testCanAddGroupMembers() {
        val group = boClient.conversations.newGroup(listOf(alix.walletAddress))
        group.addMembers(listOf(caro.walletAddress))
        assertEquals(
            group.memberAddresses().sorted(),
            listOf(
                caro.walletAddress.lowercase(),
                alix.walletAddress.lowercase(),
                bo.walletAddress.lowercase()
            ).sorted()
        )
    }

    @Test
    fun testCanRemoveGroupMembers() {
        val group = boClient.conversations.newGroup(
            listOf(
                alix.walletAddress,
                caro.walletAddress
            )
        )
        group.removeMembers(listOf(caro.walletAddress))
        assertEquals(
            group.memberAddresses().sorted(),
            listOf(
                alix.walletAddress.lowercase(),
                bo.walletAddress.lowercase()
            ).sorted()
        )
    }

    @Test
    fun testCanRemoveGroupMembersWhenNotCreator() {
        boClient.conversations.newGroup(
            listOf(
                alix.walletAddress,
                caro.walletAddress
            )
        )
        runBlocking { alixClient.conversations.syncGroups() }
        val group = alixClient.conversations.listGroups().first()
        group.removeMembers(listOf(caro.walletAddress))
        assertEquals(
            group.memberAddresses().sorted(),
            listOf(
                alix.walletAddress.lowercase(),
                bo.walletAddress.lowercase()
            ).sorted()
        )
    }

    @Test
    fun testIsActiveReturnsCorrectly() {
        val group = boClient.conversations.newGroup(
            listOf(
                alix.walletAddress,
                caro.walletAddress
            )
        )
        runBlocking { caroClient.conversations.syncGroups() }
        val caroGroup = caroClient.conversations.listGroups().first()
        runBlocking { caroGroup.sync() }
        assert(caroGroup.isActive())
        assert(group.isActive())
        group.removeMembers(listOf(caro.walletAddress))
        runBlocking { caroGroup.sync() }
        assert(group.isActive())
        assert(!caroGroup.isActive())
    }

    @Test
    fun testCanListGroups() {
        boClient.conversations.newGroup(listOf(alix.walletAddress))
        boClient.conversations.newGroup(listOf(caro.walletAddress))
        val groups = boClient.conversations.listGroups()
        assertEquals(groups.size, 2)
    }

    @Test
    fun testCanListGroupsAndConversations() {
        boClient.conversations.newGroup(listOf(alix.walletAddress))
        boClient.conversations.newGroup(listOf(caro.walletAddress))
        boClient.conversations.newConversation(alix.walletAddress)
        val convos = boClient.conversations.list(includeGroups = true)
        assertEquals(convos.size, 3)
    }

    @Test
    fun testCannotSendMessageToGroupMemberNotOnV3() {
        var fakeApiClient = FakeApiClient()
        val chuxAccount = PrivateKeyBuilder()
        val chux: PrivateKey = chuxAccount.getPrivateKey()
        val chuxClient: Client = Client().create(account = chuxAccount, apiClient = fakeApiClient)

        assertThrows("Recipient not on network", XMTPException::class.java) {
            boClient.conversations.newGroup(listOf(chux.walletAddress))
        }
    }

    @Test
    fun testCannotStartGroupWithSelf() {
        assertThrows("Recipient is sender", XMTPException::class.java) {
            boClient.conversations.newGroup(listOf(bo.walletAddress))
        }
    }

    @Test
    fun testCannotStartEmptyGroupChat() {
        assertThrows("Cannot start an empty group chat.", XMTPException::class.java) {
            boClient.conversations.newGroup(listOf())
        }
    }

    @Test
    fun testCanSendMessageToGroup() {
        val group = boClient.conversations.newGroup(listOf(alix.walletAddress))
        group.send("howdy")
        group.send("gm")
        runBlocking { group.sync() }
        assertEquals(group.messages().first().body, "gm")
        assertEquals(group.messages().size, 3)

        runBlocking { alixClient.conversations.syncGroups() }
        val sameGroup = alixClient.conversations.listGroups().last()
        runBlocking { sameGroup.sync() }
        assertEquals(sameGroup.messages().size, 2)
        assertEquals(sameGroup.messages().first().body, "gm")
    }

    @Test
    fun testCanSendContentTypesToGroup() {
        Client.register(codec = ReactionCodec())

        val group = boClient.conversations.newGroup(listOf(alix.walletAddress))
        group.send("gm")
        runBlocking { group.sync() }
        val messageToReact = group.messages()[0]

        val reaction = Reaction(
            reference = messageToReact.id,
            action = ReactionAction.Added,
            content = "U+1F603",
            schema = ReactionSchema.Unicode
        )

        group.send(content = reaction, options = SendOptions(contentType = ContentTypeReaction))
        runBlocking { group.sync() }

        val messages = group.messages()
        assertEquals(messages.size, 3)
        val content: Reaction? = messages.first().content()
        assertEquals("U+1F603", content?.content)
        assertEquals(messageToReact.id, content?.reference)
        assertEquals(ReactionAction.Added, content?.action)
        assertEquals(ReactionSchema.Unicode, content?.schema)
    }

    @Test
    fun testCanStreamGroupMessages() = kotlinx.coroutines.test.runTest {
        val group = boClient.conversations.newGroup(listOf(alix.walletAddress.lowercase()))
        group.streamMessages().test {
            group.send("hi")
            assertEquals("hi", awaitItem().body)
            group.send("hi again")
            assertEquals("hi again", awaitItem().body)
        }
    }

    @Test
    fun testCanStreamDecryptedGroupMessages() = kotlinx.coroutines.test.runTest {
        val group = boClient.conversations.newGroup(listOf(alix.walletAddress))

        group.streamDecryptedMessages().test {
            group.send("hi")
            assertEquals("hi", awaitItem().encodedContent.content.toStringUtf8())
            group.send("hi again")
            assertEquals("hi again", awaitItem().encodedContent.content.toStringUtf8())
        }
    }

    @Test
    fun testCanStreamGroups() = kotlinx.coroutines.test.runTest {
        boClient.conversations.streamGroups().test {
            val group =
                alixClient.conversations.newGroup(listOf(bo.walletAddress))
            assertEquals(group.id.toHex(), awaitItem().id.toHex())
            val group2 =
                caroClient.conversations.newGroup(listOf(bo.walletAddress))
            assertEquals(group2.id.toHex(), awaitItem().id.toHex())
        }
    }

    @Test
    fun testCanStreamGroupsAndConversations() = kotlinx.coroutines.test.runTest {
        boClient.conversations.streamAll().test {
            val group =
                caroClient.conversations.newGroup(listOf(bo.walletAddress))
            assertEquals(group.id.toHex(), awaitItem().topic)
            val conversation =
                boClient.conversations.newConversation(alix.walletAddress)
            assertEquals(conversation.topic, awaitItem().topic)
        }
    }
}
