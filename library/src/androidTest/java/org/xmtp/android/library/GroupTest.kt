package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
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
import org.xmtp.android.library.Conversations.ConversationType
import org.xmtp.android.library.codecs.ContentTypeGroupUpdated
import org.xmtp.android.library.codecs.ContentTypeReaction
import org.xmtp.android.library.codecs.GroupUpdatedCodec
import org.xmtp.android.library.codecs.Reaction
import org.xmtp.android.library.codecs.ReactionAction
import org.xmtp.android.library.codecs.ReactionCodec
import org.xmtp.android.library.codecs.ReactionSchema
import org.xmtp.android.library.libxmtp.Message.MessageDeliveryStatus
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
    fun testCanCreateAGroupWithDefaultPermissions() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(listOf(alix.walletAddress))
        }
        runBlocking {
            alixClient.conversations.sync()
            boGroup.sync()
        }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }
        assert(boGroup.id.isNotEmpty())
        assert(alixGroup.id.isNotEmpty())

        runBlocking {
            alixGroup.addMembers(listOf(caro.walletAddress))
            boGroup.sync()
        }
        assertEquals(runBlocking { alixGroup.members().size }, 3)
        assertEquals(runBlocking { boGroup.members().size }, 3)

        // All members also defaults remove to admin only now.
        assertThrows(XMTPException::class.java) {
            runBlocking {
                alixGroup.removeMembers(listOf(caro.walletAddress))
                boGroup.sync()
            }
        }

        assertEquals(runBlocking { alixGroup.members().size }, 3)
        assertEquals(runBlocking { boGroup.members().size }, 3)

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
        runBlocking { alixClient.conversations.sync() }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }
        assert(boGroup.id.isNotEmpty())
        assert(alixGroup.id.isNotEmpty())

        runBlocking {
            assertEquals(
                boClient.preferences.consentList.conversationState(boGroup.id),
                ConsentState.ALLOWED
            )
            assertEquals(
                alixClient.preferences.consentList.conversationState(alixGroup.id),
                ConsentState.UNKNOWN
            )
        }

        runBlocking {
            boGroup.addMembers(listOf(caro.walletAddress))
            alixGroup.sync()
        }

        assertEquals(runBlocking { alixGroup.members().size }, 3)
        assertEquals(runBlocking { boGroup.members().size }, 3)

        assertThrows(XMTPException::class.java) {
            runBlocking { alixGroup.removeMembers(listOf(caro.walletAddress)) }
        }
        runBlocking { boGroup.sync() }

        assertEquals(runBlocking { alixGroup.members().size }, 3)
        assertEquals(runBlocking { boGroup.members().size }, 3)
        runBlocking {
            boGroup.removeMembers(listOf(caro.walletAddress))
            alixGroup.sync()
        }

        assertEquals(runBlocking { alixGroup.members().size }, 2)
        assertEquals(runBlocking { boGroup.members().size }, 2)

        assertThrows(XMTPException::class.java) {
            runBlocking { alixGroup.addMembers(listOf(caro.walletAddress)) }
        }
        runBlocking { boGroup.sync() }

        assertEquals(runBlocking { alixGroup.members().size }, 2)
        assertEquals(runBlocking { boGroup.members().size }, 2)

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
            runBlocking { group.members().map { it.inboxId }.sorted() },
            listOf(
                caroClient.inboxId,
                alixClient.inboxId,
                boClient.inboxId
            ).sorted()
        )

        assertEquals(
            runBlocking { group.peerInboxIds().sorted() },
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
            alixClient.conversations.sync()
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
            runBlocking { group.members().map { it.inboxId }.sorted() },
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
            runBlocking { group.members().map { it.inboxId }.sorted() },
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
            alixClient.conversations.sync()
        }
        val group = runBlocking {
            alixClient.conversations.sync()
            alixClient.conversations.listGroups().first()
        }
        runBlocking {
            group.removeMembers(listOf(caroClient.address))
            group.sync()
            boGroup.sync()
        }
        assertEquals(
            runBlocking { boGroup.members().map { it.inboxId }.sorted() },
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
            runBlocking { group.members().map { it.inboxId }.sorted() },
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
            runBlocking { group.members().map { it.inboxId }.sorted() },
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
        assertEquals(alixGroup.messages().size, 2)
        runBlocking { alixGroup.sync() }
        val message2 = alixGroup.messages().last()
        runBlocking { alixGroup.sync() }
        val message3 = alixGroup.messages().last()
        assertEquals(message3.id, message2.id)
        assertEquals(message3.sent.time, message2.sent.time)
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
        runBlocking { caroClient.conversations.sync() }
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
        runBlocking {
            alixClient.conversations.newGroup(
                listOf(
                    boClient.address,
                )
            )
        }
        runBlocking { boClient.conversations.sync() }
        val boGroup = runBlocking { boClient.conversations.listGroups().first() }
        assertEquals(boGroup.addedByInboxId(), alixClient.inboxId)
    }

    @Test
    fun testCanListGroups() {
        runBlocking {
            boClient.conversations.newGroup(listOf(alix.walletAddress))
            boClient.conversations.newGroup(listOf(caro.walletAddress))
            boClient.conversations.sync()
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
            boClient.conversations.sync()
        }
        val convos = runBlocking { boClient.conversations.list() }
        assertEquals(convos.size, 3)
    }

    @Test
    fun testCannotSendMessageToGroupMemberNotOnV3() {
        val chuxAccount = PrivateKeyBuilder()
        val chux: PrivateKey = chuxAccount.getPrivateKey()

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
        runBlocking {
            val group = boClient.conversations.newGroup(listOf(alix.walletAddress))
            group.send("howdy")
            group.send("gm")
            group.sync()
            assertEquals(group.consentState(), ConsentState.ALLOWED)
            assertEquals(
                boClient.preferences.consentList.conversationState(group.id),
                ConsentState.ALLOWED
            )
        }
    }

    @Test
    fun testCanStreamAndUpdateNameWithoutForkingGroup() {
        val firstMsgCheck = 2
        val secondMsgCheck = 5
        var messageCallbacks = 0

        val job = CoroutineScope(Dispatchers.IO).launch {
            boClient.conversations.streamAllMessages().collect { _ ->
                messageCallbacks++
            }
        }
        Thread.sleep(1000)

        val alixGroup = runBlocking { alixClient.conversations.newGroup(listOf(bo.walletAddress)) }

        runBlocking {
            alixGroup.send("hello1")
            alixGroup.updateGroupName("hello")
            boClient.conversations.sync()
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
        job.cancel()
    }

    @Test
    fun testsCanListGroupsFiltered() {
        runBlocking { boClient.conversations.findOrCreateDm(caro.walletAddress) }
        runBlocking { boClient.conversations.newGroup(listOf(caro.walletAddress)) }
        val group =
            runBlocking { boClient.conversations.newGroup(listOf(caro.walletAddress)) }
        assertEquals(runBlocking { boClient.conversations.listGroups().size }, 2)
        assertEquals(
            runBlocking { boClient.conversations.listGroups(consentState = ConsentState.ALLOWED).size },
            2
        )
        runBlocking { group.updateConsentState(ConsentState.DENIED) }
        assertEquals(
            runBlocking { boClient.conversations.listGroups(consentState = ConsentState.ALLOWED).size },
            1
        )
        assertEquals(
            runBlocking { boClient.conversations.listGroups(consentState = ConsentState.DENIED).size },
            1
        )
        assertEquals(runBlocking { boClient.conversations.listGroups().size }, 2)
    }

    @Test
    fun testCanListGroupsOrder() {
        val dm = runBlocking { boClient.conversations.findOrCreateDm(caro.walletAddress) }
        val group1 =
            runBlocking { boClient.conversations.newGroup(listOf(caro.walletAddress)) }
        val group2 =
            runBlocking { boClient.conversations.newGroup(listOf(caro.walletAddress)) }
        runBlocking { dm.send("Howdy") }
        runBlocking { group2.send("Howdy") }
        runBlocking { boClient.conversations.syncAllConversations() }
        val conversations = runBlocking { boClient.conversations.listGroups() }
        val conversationsOrdered =
            runBlocking { boClient.conversations.listGroups(order = Conversations.ConversationOrder.LAST_MESSAGE) }
        assertEquals(conversations.size, 2)
        assertEquals(conversationsOrdered.size, 2)
        assertEquals(conversations.map { it.id }, listOf(group1.id, group2.id))
        assertEquals(conversationsOrdered.map { it.id }, listOf(group2.id, group1.id))
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

        runBlocking { alixClient.conversations.sync() }
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
        assertEquals(group.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED).size, 3)
        runBlocking { group.sync() }
        assertEquals(group.messages().size, 3)
        assertEquals(group.messages(deliveryStatus = MessageDeliveryStatus.UNPUBLISHED).size, 0)
        assertEquals(group.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED).size, 3)

        runBlocking { alixClient.conversations.sync() }
        val sameGroup = runBlocking { alixClient.conversations.listGroups().last() }
        runBlocking { sameGroup.sync() }
        assertEquals(sameGroup.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED).size, 2)
    }

    @Test
    fun testCanListGroupMessagesAfter() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress)) }
        val messageId = runBlocking {
            group.send("howdy")
            group.send("gm")
        }
        val message = boClient.findMessage(messageId)
        assertEquals(group.messages().size, 3)
        assertEquals(group.messages(afterNs = message?.sentAtNs).size, 0)
        runBlocking {
            group.send("howdy")
            group.send("gm")
        }
        assertEquals(group.messages().size, 5)
        assertEquals(group.messages(afterNs = message?.sentAtNs).size, 2)

        runBlocking { alixClient.conversations.sync() }
        val sameGroup = runBlocking { alixClient.conversations.listGroups().last() }
        runBlocking { sameGroup.sync() }
        assertEquals(sameGroup.messages().size, 4)
        assertEquals(sameGroup.messages(afterNs = message?.sentAtNs).size, 2)
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
        alixClient.conversations.sync()
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
        val conversation =
            runBlocking { caroClient.conversations.newConversation(alix.walletAddress) }

        runBlocking { alixClient.conversations.sync() }

        val allMessages = mutableListOf<DecodedMessage>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.conversations.streamAllMessages(type = ConversationType.GROUPS)
                    .collect { message ->
                        allMessages.add(message)
                    }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(2500)
        runBlocking { conversation.send(text = "conversation message") }
        for (i in 0 until 2) {
            runBlocking {
                group.send(text = "Message $i")
            }
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
    fun testCanStreamGroups() = kotlinx.coroutines.test.runTest {
        boClient.conversations.stream(type = ConversationType.GROUPS).test {
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
                alixClient.conversations.stream()
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
    fun testGroupConsent() {
        runBlocking {
            val group =
                boClient.conversations.newGroup(
                    listOf(
                        alix.walletAddress,
                        caro.walletAddress
                    )
                )
            assertEquals(
                boClient.preferences.consentList.conversationState(group.id),
                ConsentState.ALLOWED
            )
            assertEquals(group.consentState(), ConsentState.ALLOWED)

            boClient.preferences.consentList.setConsentState(
                listOf(
                    ConsentListEntry(
                        group.id,
                        EntryType.CONVERSATION_ID,
                        ConsentState.DENIED
                    )
                )
            )
            assertEquals(
                boClient.preferences.consentList.conversationState(group.id),
                ConsentState.DENIED
            )
            assertEquals(group.consentState(), ConsentState.DENIED)

            group.updateConsentState(ConsentState.ALLOWED)
            assertEquals(
                boClient.preferences.consentList.conversationState(group.id),
                ConsentState.ALLOWED
            )
            assertEquals(group.consentState(), ConsentState.ALLOWED)
        }
    }

    @Test
    fun testCanAllowAndDenyInboxId() {
        runBlocking {
            val boGroup = boClient.conversations.newGroup(listOf(alix.walletAddress))
            assertEquals(
                boClient.preferences.consentList.inboxIdState(alixClient.inboxId),
                ConsentState.UNKNOWN
            )
            boClient.preferences.consentList.setConsentState(
                listOf(
                    ConsentListEntry(
                        alixClient.inboxId,
                        EntryType.INBOX_ID,
                        ConsentState.ALLOWED
                    )
                )
            )
            var alixMember = boGroup.members().firstOrNull { it.inboxId == alixClient.inboxId }
            assertEquals(alixMember!!.consentState, ConsentState.ALLOWED)

            assertEquals(
                boClient.preferences.consentList.inboxIdState(alixClient.inboxId),
                ConsentState.ALLOWED
            )

            boClient.preferences.consentList.setConsentState(
                listOf(
                    ConsentListEntry(
                        alixClient.inboxId,
                        EntryType.INBOX_ID,
                        ConsentState.DENIED
                    )
                )
            )
            alixMember = boGroup.members().firstOrNull { it.inboxId == alixClient.inboxId }
            assertEquals(alixMember!!.consentState, ConsentState.DENIED)

            assertEquals(
                boClient.preferences.consentList.inboxIdState(alixClient.inboxId),
                ConsentState.DENIED
            )

            boClient.preferences.consentList.setConsentState(
                listOf(
                    ConsentListEntry(
                        alixClient.address,
                        EntryType.ADDRESS,
                        ConsentState.ALLOWED
                    )
                )
            )
            alixMember = boGroup.members().firstOrNull { it.inboxId == alixClient.inboxId }
            assertEquals(alixMember!!.consentState, ConsentState.ALLOWED)
            assertEquals(
                boClient.preferences.consentList.inboxIdState(alixClient.inboxId),
                ConsentState.ALLOWED
            )
            assertEquals(
                boClient.preferences.consentList.addressState(alixClient.address),
                ConsentState.ALLOWED
            )
        }
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
        runBlocking { alixClient.conversations.sync() }
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
        runBlocking { alixClient.conversations.sync() }
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
        runBlocking { alixClient.conversations.sync() }
        val alixGroup: Group = alixClient.findGroup(boGroup.id)!!
        runBlocking { assertEquals(alixGroup.consentState(), ConsentState.UNKNOWN) }
        val preparedMessageId = runBlocking { alixGroup.prepareMessage("Test text") }
        runBlocking { assertEquals(alixGroup.consentState(), ConsentState.ALLOWED) }
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

    @Test
    fun testSyncsAllGroupsInParallel() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alix.walletAddress,
                )
            )
        }
        val boGroup2 = runBlocking {
            boClient.conversations.newGroup(
                listOf(
                    alix.walletAddress,
                )
            )
        }
        runBlocking { alixClient.conversations.sync() }
        val alixGroup: Group = alixClient.findGroup(boGroup.id)!!
        val alixGroup2: Group = alixClient.findGroup(boGroup2.id)!!
        var numGroups: UInt?

        assertEquals(alixGroup.messages().size, 0)
        assertEquals(alixGroup2.messages().size, 0)

        runBlocking {
            boGroup.send("hi")
            boGroup2.send("hi")
            numGroups = alixClient.conversations.syncAllConversations()
        }

        assertEquals(alixGroup.messages().size, 1)
        assertEquals(alixGroup2.messages().size, 1)
        assertEquals(numGroups, 3u)

        runBlocking {
            boGroup2.removeMembers(listOf(alix.walletAddress))
            boGroup.send("hi")
            boGroup.send("hi")
            boGroup2.send("hi")
            boGroup2.send("hi")
            numGroups = alixClient.conversations.syncAllConversations()
            Thread.sleep(2000)
        }

        assertEquals(alixGroup.messages().size, 3)
        assertEquals(alixGroup2.messages().size, 2)
        // First syncAllGroups after remove includes the group you're removed from
        assertEquals(numGroups, 3u)

        runBlocking {
            numGroups = alixClient.conversations.syncAllConversations()
        }
        // Next syncAllGroups will not include the inactive group
        assertEquals(numGroups, 2u)
    }
}
