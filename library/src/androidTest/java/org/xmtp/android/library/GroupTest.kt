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
import org.xmtp.android.library.codecs.ContentTypeGroupUpdated
import org.xmtp.android.library.codecs.ContentTypeReaction
import org.xmtp.android.library.codecs.GroupUpdatedCodec
import org.xmtp.android.library.codecs.Reaction
import org.xmtp.android.library.codecs.ReactionAction
import org.xmtp.android.library.codecs.ReactionCodec
import org.xmtp.android.library.codecs.ReactionSchema
import org.xmtp.android.library.messages.DecryptedMessage
import org.xmtp.android.library.messages.MessageDeliveryStatus
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.mls.message.contents.TranscriptMessages
import uniffi.xmtpv3.org.xmtp.android.library.libxmtp.GroupPermissionPreconfiguration
import uniffi.xmtpv3.org.xmtp.android.library.libxmtp.PermissionOption

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
                    enableV3 = true,
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
        val boGroup = runBlocking {
            boClient.conversations.newGroup(listOf(alix.walletAddress))
        }
        runBlocking {
            alixClient.conversations.syncGroups()
            boGroup.sync()
        }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }
        assert(boGroup.id.isNotEmpty())
        assert(alixGroup.id.isNotEmpty())

        runBlocking {
            alixGroup.addMembers(listOf(caro.walletAddress))
            boGroup.sync()
        }
        assertEquals(alixGroup.members().size, 3)
        assertEquals(boGroup.members().size, 3)

        // All members also defaults remove to admin only now.
        assertThrows(XMTPException::class.java) {
            runBlocking {
                alixGroup.removeMembers(listOf(caro.walletAddress))
                boGroup.sync()
            }
        }

        assertEquals(alixGroup.members().size, 3)
        assertEquals(boGroup.members().size, 3)

        assertEquals(boGroup.permissionPolicySet().addMemberPolicy, PermissionOption.Allow)
        assertEquals(alixGroup.permissionPolicySet().addMemberPolicy, PermissionOption.Allow)
        assertEquals(boGroup.isSuperAdmin(boClient.inboxId), true)
        assertEquals(boGroup.isSuperAdmin(alixClient.inboxId), false)
        assertEquals(alixGroup.isSuperAdmin(boClient.inboxId), true)
        assertEquals(alixGroup.isSuperAdmin(alixClient.inboxId), false)
        // can not fetch creator ID. See https://github.com/xmtp/libxmtp/issues/788
//       assert(boGroup.isCreator())
        assert(!alixGroup.isCreator())
    }

    @Test
    fun testCanCreateAGroupWithAdminPermissions() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(
                listOf(alix.walletAddress),
                permissions = GroupPermissionPreconfiguration.ADMIN_ONLY
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

        assertEquals(alixGroup.members().size, 3)
        assertEquals(boGroup.members().size, 3)

        assertThrows(XMTPException::class.java) {
            runBlocking { alixGroup.removeMembers(listOf(caro.walletAddress)) }
        }
        runBlocking { boGroup.sync() }

        assertEquals(alixGroup.members().size, 3)
        assertEquals(boGroup.members().size, 3)
        runBlocking {
            boGroup.removeMembers(listOf(caro.walletAddress))
            alixGroup.sync()
        }

        assertEquals(alixGroup.members().size, 2)
        assertEquals(boGroup.members().size, 2)

        assertThrows(XMTPException::class.java) {
            runBlocking { alixGroup.addMembers(listOf(caro.walletAddress)) }
        }
        runBlocking { boGroup.sync() }

        assertEquals(alixGroup.members().size, 2)
        assertEquals(boGroup.members().size, 2)

        assertEquals(boGroup.permissionPolicySet().addMemberPolicy, PermissionOption.Admin)
        assertEquals(alixGroup.permissionPolicySet().addMemberPolicy, PermissionOption.Admin)
        assertEquals(boGroup.isSuperAdmin(boClient.inboxId), true)
        assertEquals(boGroup.isSuperAdmin(alixClient.inboxId), false)
        assertEquals(alixGroup.isSuperAdmin(boClient.inboxId), true)
        assertEquals(alixGroup.isSuperAdmin(alixClient.inboxId), false)
        // can not fetch creator ID. See https://github.com/xmtp/libxmtp/issues/788
//       assert(boGroup.isCreator())
        assert(!alixGroup.isCreator())
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
            group.members().map { it.inboxId }.sorted(),
            listOf(
                caroClient.inboxId,
                alixClient.inboxId,
                boClient.inboxId
            ).sorted()
        )

        assertEquals(
            Conversation.Group(group).peerAddresses.sorted(),
            listOf(
                caroClient.inboxId,
                alixClient.inboxId,
            ).sorted()
        )

        assertEquals(
            group.peerInboxIds().sorted(),
            listOf(
                caroClient.inboxId,
                alixClient.inboxId,
            ).sorted()
        )
    }

    @Test
    fun testGroupMetadata() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(
                listOf(alix.walletAddress),
                groupName = "Starting Name",
                groupImageUrlSquare = "startingurl.com"
            )
        }
        runBlocking {
            assertEquals("Starting Name", boGroup.name)
            assertEquals("startingurl.com", boGroup.imageUrlSquare)
            boGroup.updateGroupName("This Is A Great Group")
            boGroup.updateGroupImageUrlSquare("thisisanewurl.com")
            boGroup.sync()
            alixClient.conversations.syncGroups()
        }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }
        runBlocking { alixGroup.sync() }
        assertEquals("This Is A Great Group", boGroup.name)
        assertEquals("This Is A Great Group", alixGroup.name)
        assertEquals("thisisanewurl.com", boGroup.imageUrlSquare)
        assertEquals("thisisanewurl.com", alixGroup.imageUrlSquare)
    }

    @Test
    fun testCanAddGroupMembers() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress)) }
        runBlocking { group.addMembers(listOf(caro.walletAddress)) }
        assertEquals(
            group.members().map { it.inboxId }.sorted(),
            listOf(
                caroClient.inboxId,
                alixClient.inboxId,
                boClient.inboxId
            ).sorted()
        )
    }

    @Test
    fun testCanRemoveGroupMembers() {
        val group = runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alixClient.address,
                    caroClient.address
                )
            )
        }
        runBlocking { group.removeMembers(listOf(caro.walletAddress)) }
        assertEquals(
            group.members().map { it.inboxId }.sorted(),
            listOf(
                alixClient.inboxId,
                boClient.inboxId
            ).sorted()
        )
    }

    @Test
    fun testCanRemoveGroupMembersWhenNotCreator() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alixClient.address,
                    caroClient.address
                )
            )
        }
        runBlocking {
            boGroup.addAdmin(alixClient.inboxId)
            alixClient.conversations.syncGroups()
        }
        val group = runBlocking {
            alixClient.conversations.syncGroups()
            alixClient.conversations.listGroups().first()
        }
        runBlocking {
            group.removeMembers(listOf(caroClient.address))
            group.sync()
            boGroup.sync()
        }
        assertEquals(
            boGroup.members().map { it.inboxId }.sorted(),
            listOf(
                alixClient.inboxId,
                boClient.inboxId
            ).sorted()
        )
    }

    fun testCanAddGroupMemberIds() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress)) }
        runBlocking { group.addMembersByInboxId(listOf(caroClient.inboxId)) }
        assertEquals(
            group.members().map { it.inboxId }.sorted(),
            listOf(
                caroClient.inboxId,
                alixClient.inboxId,
                boClient.inboxId
            ).sorted()
        )
    }

    @Test
    fun testCanRemoveGroupMemberIds() {
        val group = runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alixClient.address,
                    caroClient.address
                )
            )
        }
        runBlocking { group.removeMembersByInboxId(listOf(caroClient.inboxId)) }
        assertEquals(
            group.members().map { it.inboxId }.sorted(),
            listOf(
                alixClient.inboxId,
                boClient.inboxId
            ).sorted()
        )
    }

    @Test
    fun testMessageTimeIsCorrect() {
        val alixGroup = runBlocking { alixClient.conversations.newGroup(listOf(boClient.address)) }
        runBlocking { alixGroup.send("Hello") }
        assertEquals(alixGroup.decryptedMessages().size, 2)
        runBlocking { alixGroup.sync() }
        val message2 = alixGroup.decryptedMessages().last()
        runBlocking { alixGroup.sync() }
        val message3 = alixGroup.decryptedMessages().last()
        assertEquals(message3.id, message2.id)
        assertEquals(message3.sentAt.time, message2.sentAt.time)
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
                    boClient.address,
                )
            )
        }
        runBlocking { boClient.conversations.syncGroups() }
        val boGroup = runBlocking { boClient.conversations.listGroups().first() }
        assertEquals(boGroup.addedByInboxId(), alixClient.inboxId)
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
    fun testCanStartEmptyGroupChat() {
        val group = runBlocking { boClient.conversations.newGroup(listOf()) }
        assert(group.id.isNotEmpty())
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
    fun testCanStreamAndUpdateNameWithoutForkingGroup() {
        val firstMsgCheck = 2
        val secondMsgCheck = 5
        var messageCallbacks = 0

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                boClient.conversations.streamAllGroupMessages().collect { message ->
                    messageCallbacks++
                }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(1000)

        val alixGroup = runBlocking { alixClient.conversations.newGroup(listOf(bo.walletAddress)) }

        runBlocking {
            alixGroup.send("hello1")
            alixGroup.updateGroupName("hello")
            boClient.conversations.syncGroups()
        }

        val boGroups = runBlocking { boClient.conversations.listGroups() }
        assertEquals(boGroups.size, 1)
        val boGroup = boGroups[0]
        runBlocking {
            boGroup.sync()
        }

        val boMessages1 = boGroup.messages()
        assertEquals(boMessages1.size, firstMsgCheck)

        runBlocking {
            boGroup.send("hello2")
            boGroup.send("hello3")
            alixGroup.sync()
        }
        Thread.sleep(1000)
        val alixMessages = alixGroup.messages()
        assertEquals(alixMessages.size, secondMsgCheck)
        runBlocking {
            alixGroup.send("hello4")
            boGroup.sync()
        }

        val boMessages2 = boGroup.messages()
        assertEquals(boMessages2.size, secondMsgCheck)

        Thread.sleep(1000)

        assertEquals(secondMsgCheck, messageCallbacks)
    }

    @Test
    fun testCanSendMessageToGroup() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress)) }
        runBlocking { group.send("howdy") }
        val messageId = runBlocking { group.send("gm") }
        runBlocking { group.sync() }
        assertEquals(group.messages().first().body, "gm")
        assertEquals(group.messages().first().id, messageId)
        assertEquals(group.messages().first().deliveryStatus, MessageDeliveryStatus.PUBLISHED)
        assertEquals(group.messages().size, 3)

        runBlocking { alixClient.conversations.syncGroups() }
        val sameGroup = runBlocking { alixClient.conversations.listGroups().last() }
        runBlocking { sameGroup.sync() }
        assertEquals(sameGroup.messages().size, 2)
        assertEquals(sameGroup.messages().first().body, "gm")
    }

    @Test
    fun testCanListGroupMessages() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress)) }
        runBlocking {
            group.send("howdy")
            group.send("gm")
        }

        assertEquals(group.messages().size, 3)
        assertEquals(group.messages(deliveryStatus = MessageDeliveryStatus.UNPUBLISHED).size, 2)
        assertEquals(group.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED).size, 1)
        runBlocking { group.sync() }
        assertEquals(group.messages().size, 3)
        assertEquals(group.messages(deliveryStatus = MessageDeliveryStatus.UNPUBLISHED).size, 0)
        assertEquals(group.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED).size, 3)

        runBlocking { alixClient.conversations.syncGroups() }
        val sameGroup = runBlocking { alixClient.conversations.listGroups().last() }
        runBlocking { sameGroup.sync() }
        assertEquals(sameGroup.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED).size, 2)
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
        Client.register(codec = GroupUpdatedCodec())
        val membershipChange = TranscriptMessages.GroupUpdated.newBuilder().build()

        val group = boClient.conversations.newGroup(listOf(alix.walletAddress.lowercase()))
        alixClient.conversations.syncGroups()
        val alixGroup = alixClient.conversations.listGroups().first()
        group.streamMessages().test {
            alixGroup.send("hi")
            assertEquals("hi", awaitItem().body)
            alixGroup.send(
                content = membershipChange,
                options = SendOptions(contentType = ContentTypeGroupUpdated),
            )
            alixGroup.send("hi again")
            assertEquals("hi again", awaitItem().body)
        }
    }

    @Test
    fun testCanStreamAllGroupMessages() {
        val group = runBlocking { caroClient.conversations.newGroup(listOf(alix.walletAddress)) }
        runBlocking { alixClient.conversations.syncGroups() }

        val allMessages = mutableListOf<DecodedMessage>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.conversations.streamAllGroupMessages().collect { message ->
                    allMessages.add(message)
                }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(2500)

        for (i in 0 until 2) {
            runBlocking { group.send(text = "Message $i") }
            Thread.sleep(100)
        }
        assertEquals(2, allMessages.size)

        val caroGroup =
            runBlocking { caroClient.conversations.newGroup(listOf(alixClient.address)) }
        Thread.sleep(2500)

        for (i in 0 until 2) {
            runBlocking { caroGroup.send(text = "Message $i") }
            Thread.sleep(100)
        }

        assertEquals(4, allMessages.size)

        job.cancel()
    }

    @Test
    fun testCanStreamAllMessages() {
        val group = runBlocking { caroClient.conversations.newGroup(listOf(alix.walletAddress)) }
        val conversation =
            runBlocking { boClient.conversations.newConversation(alix.walletAddress) }
        runBlocking { alixClient.conversations.syncGroups() }

        val allMessages = mutableListOf<DecodedMessage>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.conversations.streamAllMessages(includeGroups = true)
                    .collect { message ->
                        allMessages.add(message)
                    }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(2500)

        runBlocking {
            group.send("hi")
            conversation.send("hi")
        }

        Thread.sleep(1000)

        assertEquals(2, allMessages.size)

        job.cancel()
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
    fun testCanStreamAllDecryptedGroupMessages() {
        val group = runBlocking { caroClient.conversations.newGroup(listOf(alix.walletAddress)) }
        runBlocking { alixClient.conversations.syncGroups() }

        val allMessages = mutableListOf<DecryptedMessage>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.conversations.streamAllGroupDecryptedMessages().collect { message ->
                    allMessages.add(message)
                }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(2500)

        for (i in 0 until 2) {
            runBlocking { group.send(text = "Message $i") }
            Thread.sleep(100)
        }
        assertEquals(2, allMessages.size)

        val caroGroup =
            runBlocking { caroClient.conversations.newGroup(listOf(alixClient.address)) }
        Thread.sleep(2500)

        for (i in 0 until 2) {
            runBlocking { caroGroup.send(text = "Message $i") }
            Thread.sleep(100)
        }

        assertEquals(4, allMessages.size)

        job.cancel()
    }

    @Test
    fun testCanStreamAllDecryptedMessages() {
        val group = runBlocking { caroClient.conversations.newGroup(listOf(alix.walletAddress)) }
        val conversation =
            runBlocking { boClient.conversations.newConversation(alix.walletAddress) }
        runBlocking { alixClient.conversations.syncGroups() }

        val allMessages = mutableListOf<DecryptedMessage>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.conversations.streamAllDecryptedMessages(includeGroups = true)
                    .collect { message ->
                        allMessages.add(message)
                    }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(2500)

        runBlocking {
            group.send("hi")
            conversation.send("hi")
        }

        Thread.sleep(1000)

        assertEquals(2, allMessages.size)

        job.cancel()
    }

    @Test
    fun testCanStreamGroups() = kotlinx.coroutines.test.runTest {
        boClient.conversations.streamGroups().test {
            val group =
                alixClient.conversations.newGroup(listOf(bo.walletAddress))
            assertEquals(group.id, awaitItem().id)
            val group2 =
                caroClient.conversations.newGroup(listOf(bo.walletAddress))
            assertEquals(group2.id, awaitItem().id)
        }
    }

    @Test
    fun testCanStreamGroupsAndConversations() {
        val allMessages = mutableListOf<String>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.conversations.streamAll()
                    .collect { message ->
                        allMessages.add(message.topic)
                    }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(2500)

        runBlocking {
            alixClient.conversations.newConversation(bo.walletAddress)
            Thread.sleep(2500)
            caroClient.conversations.newGroup(listOf(alix.walletAddress))
        }

        Thread.sleep(2500)

        assertEquals(2, allMessages.size)

        job.cancel()
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

        runBlocking { boClient.contacts.allowGroups(listOf(group.id)) }

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

        runBlocking { boClient.contacts.denyGroups(listOf(group.id)) }

        result = boClient.contacts.isGroupDenied(group.id)
        assert(result)
    }

    @Test
    fun testCanAllowAndDenyInboxId() {
        assert(!boClient.contacts.isInboxAllowed(alixClient.inboxId))
        assert(!boClient.contacts.isInboxDenied(alixClient.inboxId))

        runBlocking { boClient.contacts.allowInboxes(listOf(alixClient.inboxId)) }

        assert(boClient.contacts.isInboxAllowed(alixClient.inboxId))
        assert(!boClient.contacts.isInboxDenied(alixClient.inboxId))

        runBlocking { boClient.contacts.denyInboxes(listOf(alixClient.inboxId)) }

        assert(!boClient.contacts.isInboxAllowed(alixClient.inboxId))
        assert(boClient.contacts.isInboxDenied(alixClient.inboxId))
    }

    @Test
    fun testCanFetchGroupById() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alix.walletAddress,
                    caro.walletAddress
                )
            )
        }
        runBlocking { alixClient.conversations.syncGroups() }
        val alixGroup = alixClient.findGroup(boGroup.id)

        assertEquals(alixGroup?.id, boGroup.id)
    }

    @Test
    fun testCanFetchMessageById() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alix.walletAddress,
                    caro.walletAddress
                )
            )
        }
        val boMessageId = runBlocking { boGroup.send("Hello") }
        runBlocking { alixClient.conversations.syncGroups() }
        val alixGroup = alixClient.findGroup(boGroup.id)
        runBlocking { alixGroup?.sync() }
        val alixMessage = alixClient.findMessage(boMessageId)

        assertEquals(alixMessage?.id, boMessageId)
    }

    @Test
    fun testUnpublishedMessages() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alix.walletAddress,
                    caro.walletAddress
                )
            )
        }
        runBlocking { alixClient.conversations.syncGroups() }
        val alixGroup: Group = alixClient.findGroup(boGroup.id)!!
        assert(!alixClient.contacts.isGroupAllowed(boGroup.id))
        val preparedMessageId = runBlocking { alixGroup.prepareMessage("Test text") }
        assert(alixClient.contacts.isGroupAllowed(boGroup.id))
        assertEquals(alixGroup.messages().size, 1)
        assertEquals(alixGroup.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED).size, 0)
        assertEquals(alixGroup.messages(deliveryStatus = MessageDeliveryStatus.UNPUBLISHED).size, 1)

        runBlocking {
            alixGroup.publishMessages()
            alixGroup.sync()
        }

        assertEquals(alixGroup.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED).size, 1)
        assertEquals(alixGroup.messages(deliveryStatus = MessageDeliveryStatus.UNPUBLISHED).size, 0)
        assertEquals(alixGroup.messages().size, 1)

        val message = alixGroup.messages().first()

        assertEquals(preparedMessageId, message.id)
    }
}
