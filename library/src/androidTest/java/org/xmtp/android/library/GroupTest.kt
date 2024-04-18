package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.cash.turbine.test
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.catch
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
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress
import uniffi.xmtpv3.GroupPermissions
import uniffi.xmtpv3.org.xmtp.android.library.codecs.ContentTypeGroupMembershipChange
import uniffi.xmtpv3.org.xmtp.android.library.codecs.GroupMembershipChangeCodec
import uniffi.xmtpv3.org.xmtp.android.library.codecs.GroupMembershipChanges

@RunWith(AndroidJUnit4::class)
class GroupTest {
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

        alixClient = fixtures.aliceClient
        boClient = fixtures.bobClient
        caroClient = fixtures.caroClient
    }

    @Test
    fun testCanCreateAGroupWithDefaultPermissions() {
        val boGroup = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress)) }
        runBlocking { alixClient.conversations.syncGroups() }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }
        assert(boGroup.id.isNotEmpty())
        assert(alixGroup.id.isNotEmpty())

        runBlocking {
            alixGroup.addMembers(listOf(caro.walletAddress))
            boGroup.sync()
        }
        assertEquals(alixGroup.memberAddresses().size, 3)
        assertEquals(boGroup.memberAddresses().size, 3)

        runBlocking {
            alixGroup.removeMembers(listOf(caro.walletAddress))
            boGroup.sync()
        }
        assertEquals(alixGroup.memberAddresses().size, 2)
        assertEquals(boGroup.memberAddresses().size, 2)

        runBlocking {
            boGroup.addMembers(listOf(caro.walletAddress))
            alixGroup.sync()
        }
        assertEquals(alixGroup.memberAddresses().size, 3)
        assertEquals(boGroup.memberAddresses().size, 3)

        assertEquals(boGroup.permissionLevel(), GroupPermissions.EVERYONE_IS_ADMIN)
        assertEquals(alixGroup.permissionLevel(), GroupPermissions.EVERYONE_IS_ADMIN)
        assertEquals(boGroup.adminAddress().lowercase(), boClient.address.lowercase())
        assertEquals(alixGroup.adminAddress().lowercase(), boClient.address.lowercase())
        assert(boGroup.isAdmin())
        assert(!alixGroup.isAdmin())
    }

    @Test
    fun testCanCreateAGroupWithAdminPermissions() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(
                listOf(alix.walletAddress),
                permissions = GroupPermissions.GROUP_CREATOR_IS_ADMIN
            )
        }
        runBlocking { alixClient.conversations.syncGroups() }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }
        assert(boGroup.id.isNotEmpty())
        assert(alixGroup.id.isNotEmpty())

        assertEquals(boClient.contacts.consentList.groupState(boGroup.id), ConsentState.ALLOWED)
        assertEquals(alixClient.contacts.consentList.groupState(alixGroup.id), ConsentState.UNKNOWN)

        runBlocking {
            boGroup.addMembers(listOf(caro.walletAddress))
            alixGroup.sync()
        }
        assertEquals(alixGroup.memberAddresses().size, 3)
        assertEquals(boGroup.memberAddresses().size, 3)

        assertThrows(XMTPException::class.java) {
            runBlocking { alixGroup.removeMembers(listOf(caro.walletAddress)) }
        }
        runBlocking { boGroup.sync() }
        assertEquals(alixGroup.memberAddresses().size, 3)
        assertEquals(boGroup.memberAddresses().size, 3)
        runBlocking {
            boGroup.removeMembers(listOf(caro.walletAddress))
            alixGroup.sync()
        }
        assertEquals(alixGroup.memberAddresses().size, 2)
        assertEquals(boGroup.memberAddresses().size, 2)

        assertThrows(XMTPException::class.java) {
            runBlocking { alixGroup.addMembers(listOf(caro.walletAddress)) }
        }
        runBlocking { boGroup.sync() }
        assertEquals(alixGroup.memberAddresses().size, 2)
        assertEquals(boGroup.memberAddresses().size, 2)

        assertEquals(boGroup.permissionLevel(), GroupPermissions.GROUP_CREATOR_IS_ADMIN)
        assertEquals(alixGroup.permissionLevel(), GroupPermissions.GROUP_CREATOR_IS_ADMIN)
        assertEquals(boGroup.adminAddress().lowercase(), boClient.address.lowercase())
        assertEquals(alixGroup.adminAddress().lowercase(), boClient.address.lowercase())
        assert(boGroup.isAdmin())
        assert(!alixGroup.isAdmin())
    }

    @Test
    fun testCanListGroupMembers() {
        val group = runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alix.walletAddress,
                    caro.walletAddress
                )
            )
        }
        assertEquals(
            group.memberAddresses().sorted(),
            listOf(
                caro.walletAddress.lowercase(),
                alix.walletAddress.lowercase(),
                bo.walletAddress.lowercase()
            ).sorted()
        )

        assertEquals(
            Conversation.Group(group).peerAddresses.sorted(),
            listOf(
                caro.walletAddress.lowercase(),
                alix.walletAddress.lowercase(),
            ).sorted()
        )
    }

    @Test
    fun testCanAddGroupMembers() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress)) }
        runBlocking { group.addMembers(listOf(caro.walletAddress)) }
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
        val group = runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alix.walletAddress,
                    caro.walletAddress
                )
            )
        }
        runBlocking { group.removeMembers(listOf(caro.walletAddress)) }
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
        runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alix.walletAddress,
                    caro.walletAddress
                )
            )
        }
        runBlocking { alixClient.conversations.syncGroups() }
        val group = runBlocking { alixClient.conversations.listGroups().first() }
        runBlocking { group.removeMembers(listOf(caro.walletAddress)) }
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
        val group = runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alix.walletAddress,
                    caro.walletAddress
                )
            )
        }
        runBlocking { caroClient.conversations.syncGroups() }
        val caroGroup = runBlocking { caroClient.conversations.listGroups().first() }
        runBlocking { caroGroup.sync() }
        assert(caroGroup.isActive())
        assert(group.isActive())
        runBlocking {
            group.removeMembers(listOf(caro.walletAddress))
            caroGroup.sync()
        }
        assert(group.isActive())
        assert(!caroGroup.isActive())
    }

    @Test
    fun testAddedByAddress() {
        val group = runBlocking {
            alixClient.conversations.newGroup(
                listOf(
                    bo.walletAddress,
                )
            )
        }
        runBlocking { boClient.conversations.syncGroups() }
        val boGroup = runBlocking { boClient.conversations.listGroups().first() }
        assertEquals(boGroup.addedByAddress().lowercase(), alix.walletAddress.lowercase())
    }

    @Test
    fun testCanListGroups() {
        runBlocking {
            boClient.conversations.newGroup(listOf(alix.walletAddress))
            boClient.conversations.newGroup(listOf(caro.walletAddress))
        }
        val groups = runBlocking { boClient.conversations.listGroups() }
        assertEquals(groups.size, 2)
    }

    @Test
    fun testCanListGroupsAndConversations() {
        runBlocking {
            boClient.conversations.newGroup(listOf(alix.walletAddress))
            boClient.conversations.newGroup(listOf(caro.walletAddress))
            boClient.conversations.newConversation(alix.walletAddress)
        }
        val convos = runBlocking { boClient.conversations.list(includeGroups = true) }
        assertEquals(convos.size, 3)
    }

    @Test
    fun testCannotSendMessageToGroupMemberNotOnV3() {
        val chuxAccount = PrivateKeyBuilder()
        val chux: PrivateKey = chuxAccount.getPrivateKey()
        Client().create(account = chuxAccount)

        assertThrows("Recipient not on network", XMTPException::class.java) {
            runBlocking { boClient.conversations.newGroup(listOf(chux.walletAddress)) }
        }
    }

    @Test
    fun testCannotStartGroupWithSelf() {
        assertThrows("Recipient is sender", XMTPException::class.java) {
            runBlocking { boClient.conversations.newGroup(listOf(bo.walletAddress)) }
        }
    }

    @Test
    fun testCannotStartEmptyGroupChat() {
        assertThrows("Cannot start an empty group chat.", XMTPException::class.java) {
            runBlocking { boClient.conversations.newGroup(listOf()) }
        }
    }

    @Test
    fun testGroupStartsWithAllowedState() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress)) }
        runBlocking { group.send("howdy") }
        runBlocking { group.send("gm") }
        runBlocking { group.sync() }
        assert(boClient.contacts.isGroupAllowed(group.id))
        assertEquals(boClient.contacts.consentList.groupState(group.id), ConsentState.ALLOWED)
    }

    @Test
    fun testCanSendMessageToGroup() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress)) }
        runBlocking { group.send("howdy") }
        runBlocking { group.send("gm") }
        runBlocking { group.sync() }
        assertEquals(group.messages().first().body, "gm")
        assertEquals(group.messages().size, 3)

        runBlocking { alixClient.conversations.syncGroups() }
        val sameGroup = runBlocking { alixClient.conversations.listGroups().last() }
        runBlocking { sameGroup.sync() }
        assertEquals(sameGroup.messages().size, 2)
        assertEquals(sameGroup.messages().first().body, "gm")
    }

    @Test
    fun testCanSendContentTypesToGroup() {
        Client.register(codec = ReactionCodec())

        val group = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress)) }
        runBlocking { group.send("gm") }
        runBlocking { group.sync() }
        val messageToReact = group.messages()[0]

        val reaction = Reaction(
            reference = messageToReact.id,
            action = ReactionAction.Added,
            content = "U+1F603",
            schema = ReactionSchema.Unicode
        )

        runBlocking {
            group.send(
                content = reaction,
                options = SendOptions(contentType = ContentTypeReaction)
            )
        }
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
        Client.register(codec = GroupMembershipChangeCodec())
        val membershipChange = GroupMembershipChanges.newBuilder().build()

        val group = boClient.conversations.newGroup(listOf(alix.walletAddress.lowercase()))
        alixClient.conversations.syncGroups()
        val alixGroup = alixClient.conversations.listGroups().first()
        group.streamMessages().test {
            alixGroup.send("hi")
            assertEquals("hi", awaitItem().body)
            alixGroup.send(
                content = membershipChange,
                options = SendOptions(contentType = ContentTypeGroupMembershipChange),
            )
            alixGroup.send("hi again")
            assertEquals("hi again", awaitItem().body)
        }
    }

    @Test
    fun testCanStreamAllGroupMessages() = kotlinx.coroutines.test.runTest {
        val group = caroClient.conversations.newGroup(listOf(alix.walletAddress))
        alixClient.conversations.syncGroups()
        val flow = alixClient.conversations.streamAllGroupMessages()
        var counter = 0
        val job = launch {
            flow.catch { e ->
                throw Exception("Error collecting flow: $e")
            }.collect { message ->
                counter++
                assertEquals("hi $counter", message.encodedContent.content.toStringUtf8())
                if (counter == 2) this.cancel()
            }
        }

        group.send("hi 1")
        group.send("hi 2")

        job.join()
    }

    @Test
    fun testCanStreamAllMessages() = kotlinx.coroutines.test.runTest {
        val group = caroClient.conversations.newGroup(listOf(alix.walletAddress))
        val conversation = boClient.conversations.newConversation(alix.walletAddress)
        alixClient.conversations.syncGroups()

        val flow = alixClient.conversations.streamAllMessages(includeGroups = true)
        var counter = 0
        val job = launch {
            flow.catch { e ->
                throw Exception("Error collecting flow: $e")
            }.collect { message ->
                counter++
                assertEquals("hi $counter", message.encodedContent.content.toStringUtf8())
                if (counter == 2) this.cancel()
            }
        }

        group.send("hi 1")
        conversation.send("hi 2")

        job.join()
    }

    @Test
    fun testCanStreamDecryptedGroupMessages() = kotlinx.coroutines.test.runTest {
        val group = boClient.conversations.newGroup(listOf(alix.walletAddress))
        alixClient.conversations.syncGroups()
        val alixGroup = alixClient.conversations.listGroups().first()
        group.streamDecryptedMessages().test {
            alixGroup.send("hi")
            assertEquals("hi", awaitItem().encodedContent.content.toStringUtf8())
            alixGroup.send("hi again")
            assertEquals("hi again", awaitItem().encodedContent.content.toStringUtf8())
        }
    }

    @Test
    fun testCanStreamAllDecryptedGroupMessages() = kotlinx.coroutines.test.runTest {
        Client.register(codec = GroupMembershipChangeCodec())
        val membershipChange = GroupMembershipChanges.newBuilder().build()
        val group = caroClient.conversations.newGroup(listOf(alix.walletAddress))
        alixClient.conversations.syncGroups()

        val flow = alixClient.conversations.streamAllGroupDecryptedMessages()
        var counter = 0
        val job = launch {
            flow.catch { e ->
                throw Exception("Error collecting flow: $e")
            }.collect { message ->
                counter++
                assertEquals("hi $counter", message.encodedContent.content.toStringUtf8())
                if (counter == 2) this.cancel()
            }
        }

        group.send("hi 1")
        group.send(
            content = membershipChange,
            options = SendOptions(contentType = ContentTypeGroupMembershipChange),
        )
        group.send("hi 2")

        job.join()
    }

    @Test
    fun testCanStreamAllDecryptedMessages() = kotlinx.coroutines.test.runTest {
        val group = caroClient.conversations.newGroup(listOf(alix.walletAddress))
        val conversation = boClient.conversations.newConversation(alix.walletAddress)
        alixClient.conversations.syncGroups()

        val flow = alixClient.conversations.streamAllDecryptedMessages(includeGroups = true)
        var counter = 0
        val job = launch {
            flow.catch { e ->
                throw Exception("Error collecting flow: $e")
            }.collect { message ->
                counter++
                assertEquals("hi $counter", message.encodedContent.content.toStringUtf8())
                if (counter == 2) this.cancel()
            }
        }

        group.send("hi 1")
        conversation.send("hi 2")

        job.join()
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
            assertEquals(group.topic, awaitItem().topic)
            val conversation =
                alixClient.conversations.newConversation(bo.walletAddress)
            assertEquals(conversation.topic, awaitItem().topic)
        }
    }

    @Test
    fun testCanAllowGroup() {
        val group = runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alix.walletAddress,
                    caro.walletAddress
                )
            )
        }

        var result = boClient.contacts.isGroupAllowed(group.id)
        assert(result)

        runBlocking { boClient.contacts.allowGroup(listOf(group.id)) }

        result = boClient.contacts.isGroupAllowed(group.id)
        assert(result)
    }

    @Test
    fun testCanDenyGroup() {
        val group = runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alix.walletAddress,
                    caro.walletAddress
                )
            )
        }
        var result = boClient.contacts.isGroupAllowed(group.id)
        assert(result)

        runBlocking { boClient.contacts.denyGroup(listOf(group.id)) }

        result = boClient.contacts.isGroupDenied(group.id)
        assert(result)
    }
}
