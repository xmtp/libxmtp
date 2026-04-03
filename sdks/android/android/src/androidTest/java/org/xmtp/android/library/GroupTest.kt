package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import app.cash.turbine.test
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.Conversations.ConversationFilterType
import org.xmtp.android.library.codecs.ContentTypeGroupUpdated
import org.xmtp.android.library.codecs.ContentTypeReaction
import org.xmtp.android.library.codecs.GroupUpdatedCodec
import org.xmtp.android.library.codecs.Reaction
import org.xmtp.android.library.codecs.ReactionAction
import org.xmtp.android.library.codecs.ReactionCodec
import org.xmtp.android.library.codecs.ReactionSchema
import org.xmtp.android.library.libxmtp.DecodedMessage
import org.xmtp.android.library.libxmtp.DecodedMessage.MessageDeliveryStatus
import org.xmtp.android.library.libxmtp.DecodedMessage.SortBy
import org.xmtp.android.library.libxmtp.DisappearingMessageSettings
import org.xmtp.android.library.libxmtp.GroupMembershipState
import org.xmtp.android.library.libxmtp.GroupPermissionPreconfiguration
import org.xmtp.android.library.libxmtp.IdentityKind
import org.xmtp.android.library.libxmtp.PermissionOption
import org.xmtp.android.library.libxmtp.PublicIdentity
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.mls.message.contents.TranscriptMessages
import uniffi.xmtpv3.FfiException

@RunWith(AndroidJUnit4::class)
class GroupTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var alixClient: Client
    private lateinit var boClient: Client
    private lateinit var caroClient: Client

    @Before
    override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
        caroClient = fixtures.caroClient
    }

    @Test
    fun testCanCreateAGroupWithDefaultPermissions() {
        val boGroup =
            runBlocking {
                fixtures.boClient.conversations.newGroup(listOf(fixtures.alixClient.inboxId))
            }
        runBlocking {
            fixtures.alixClient.conversations.sync()
            boGroup.sync()
        }
        val alixGroup =
            runBlocking {
                fixtures.alixClient.conversations
                    .listGroups()
                    .first()
            }
        assert(boGroup.id.isNotEmpty())
        assert(alixGroup.id.isNotEmpty())

        runBlocking {
            alixGroup.addMembers(listOf(fixtures.caroClient.inboxId))
            boGroup.sync()
        }
        assertEquals(runBlocking { alixGroup.members().size }, 3)
        assertEquals(runBlocking { boGroup.members().size }, 3)

        // All members also defaults remove to admin only now.
        assertThrows(XMTPException::class.java) {
            runBlocking {
                alixGroup.removeMembers(listOf(fixtures.caroClient.inboxId))
                boGroup.sync()
            }
        }

        assertEquals(runBlocking { alixGroup.members().size }, 3)
        assertEquals(runBlocking { boGroup.members().size }, 3)

        assertEquals(
            runBlocking { boGroup.permissionPolicySet().addMemberPolicy },
            PermissionOption.Allow,
        )
        assertEquals(
            runBlocking { alixGroup.permissionPolicySet().addMemberPolicy },
            PermissionOption.Allow,
        )
        assertEquals(runBlocking { boGroup.isSuperAdmin(boClient.inboxId) }, true)
        assertEquals(runBlocking { boGroup.isSuperAdmin(alixClient.inboxId) }, false)
        assertEquals(runBlocking { alixGroup.isSuperAdmin(boClient.inboxId) }, true)
        assertEquals(runBlocking { alixGroup.isSuperAdmin(alixClient.inboxId) }, false)
        // can not fetch creator ID. See https://github.com/xmtp/libxmtp/issues/788
//       assert(boGroup.isCreator())
        assert(!runBlocking { alixGroup.isCreator() })
    }

    @Test
    fun testCanCreateAGroupWithAdminPermissions() {
        val boGroup =
            runBlocking {
                boClient.conversations.newGroup(
                    listOf(alixClient.inboxId),
                    permissions = GroupPermissionPreconfiguration.ADMIN_ONLY,
                )
            }
        runBlocking { alixClient.conversations.sync() }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }
        assert(boGroup.id.isNotEmpty())
        assert(alixGroup.id.isNotEmpty())

        runBlocking {
            assertEquals(
                boClient.preferences.conversationState(boGroup.id),
                ConsentState.ALLOWED,
            )
            assertEquals(
                alixClient.preferences.conversationState(alixGroup.id),
                ConsentState.UNKNOWN,
            )
        }

        runBlocking {
            boGroup.addMembers(listOf(caroClient.inboxId))
            alixGroup.sync()
        }

        assertEquals(runBlocking { alixGroup.members().size }, 3)
        assertEquals(runBlocking { boGroup.members().size }, 3)

        assertThrows(XMTPException::class.java) {
            runBlocking { alixGroup.removeMembers(listOf(caroClient.inboxId)) }
        }
        runBlocking { boGroup.sync() }

        assertEquals(runBlocking { alixGroup.members().size }, 3)
        assertEquals(runBlocking { boGroup.members().size }, 3)
        runBlocking {
            boGroup.removeMembers(listOf(caroClient.inboxId))
            alixGroup.sync()
        }

        assertEquals(runBlocking { alixGroup.members().size }, 2)
        assertEquals(runBlocking { boGroup.members().size }, 2)

        assertThrows(XMTPException::class.java) {
            runBlocking { alixGroup.addMembers(listOf(caroClient.inboxId)) }
        }
        runBlocking { boGroup.sync() }

        assertEquals(runBlocking { alixGroup.members().size }, 2)
        assertEquals(runBlocking { boGroup.members().size }, 2)

        assertEquals(
            runBlocking { boGroup.permissionPolicySet().addMemberPolicy },
            PermissionOption.Admin,
        )
        assertEquals(
            runBlocking { alixGroup.permissionPolicySet().addMemberPolicy },
            PermissionOption.Admin,
        )
        assertEquals(runBlocking { boGroup.isSuperAdmin(boClient.inboxId) }, true)
        assertEquals(runBlocking { boGroup.isSuperAdmin(alixClient.inboxId) }, false)
        assertEquals(runBlocking { alixGroup.isSuperAdmin(boClient.inboxId) }, true)
        assertEquals(runBlocking { alixGroup.isSuperAdmin(alixClient.inboxId) }, false)
        // can not fetch creator ID. See https://github.com/xmtp/libxmtp/issues/788
//       assert(boGroup.isCreator())
        assert(!runBlocking { alixGroup.isCreator() })
    }

    @Test
    fun testCanCreateAGroupWithInboxIdsDefaultPermissions() {
        val boGroup =
            runBlocking {
                boClient.conversations.newGroupWithIdentities(
                    listOf(
                        PublicIdentity(
                            IdentityKind.ETHEREUM,
                            fixtures.alix.walletAddress,
                        ),
                    ),
                )
            }
        runBlocking {
            alixClient.conversations.sync()
            boGroup.sync()
        }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }
        assert(boGroup.id.isNotEmpty())
        assert(alixGroup.id.isNotEmpty())

        runBlocking {
            alixGroup.addMembers(listOf(caroClient.inboxId))
            boGroup.sync()
        }
        assertEquals(runBlocking { alixGroup.members().size }, 3)
        assertEquals(runBlocking { boGroup.members().size }, 3)

        // All members also defaults remove to admin only now.
        assertThrows(XMTPException::class.java) {
            runBlocking {
                alixGroup.removeMembers(listOf(caroClient.inboxId))
                boGroup.sync()
            }
        }

        assertEquals(runBlocking { alixGroup.members().size }, 3)
        assertEquals(runBlocking { boGroup.members().size }, 3)

        assertEquals(
            runBlocking { boGroup.permissionPolicySet().addMemberPolicy },
            PermissionOption.Allow,
        )
        assertEquals(
            runBlocking { alixGroup.permissionPolicySet().addMemberPolicy },
            PermissionOption.Allow,
        )
        assertEquals(runBlocking { boGroup.isSuperAdmin(boClient.inboxId) }, true)
        assertEquals(runBlocking { boGroup.isSuperAdmin(alixClient.inboxId) }, false)
        assertEquals(runBlocking { alixGroup.isSuperAdmin(boClient.inboxId) }, true)
        assertEquals(runBlocking { alixGroup.isSuperAdmin(alixClient.inboxId) }, false)
        assert(!runBlocking { alixGroup.isCreator() })
    }

    @Test
    fun testCanListGroupMembers() {
        val group =
            runBlocking {
                boClient.conversations.newGroup(
                    listOf(
                        alixClient.inboxId,
                        caroClient.inboxId,
                    ),
                )
            }
        assertEquals(
            runBlocking { group.members().map { it.inboxId }.sorted() },
            listOf(
                caroClient.inboxId,
                alixClient.inboxId,
                boClient.inboxId,
            ).sorted(),
        )

        assertEquals(
            runBlocking { group.peerInboxIds().map { it }.sorted() },
            listOf(
                caroClient.inboxId,
                alixClient.inboxId,
            ).sorted(),
        )
    }

    @Test
    fun testGroupMetadata() =
        runBlocking {
            val boGroup =
                boClient.conversations.newGroup(
                    listOf(alixClient.inboxId),
                    groupName = "Starting Name",
                    groupImageUrlSquare = "startingurl.com",
                )
            assertEquals("Starting Name", boGroup.name())
            assertEquals("startingurl.com", boGroup.imageUrl())
            boGroup.updateName("This Is A Great Group")
            boGroup.updateImageUrl("thisisanewurl.com")
            boGroup.sync()

            alixClient.conversations.sync()
            val alixGroup = alixClient.conversations.listGroups().first()
            alixGroup.sync()
            assertEquals("This Is A Great Group", boGroup.name())
            assertEquals("This Is A Great Group", alixGroup.name())
            assertEquals("thisisanewurl.com", boGroup.imageUrl())
            assertEquals("thisisanewurl.com", alixGroup.imageUrl())
        }

    @Test
    fun testCanAddGroupMembers() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alixClient.inboxId)) }
        val result = runBlocking { group.addMembers(listOf(caroClient.inboxId)) }
        assertEquals(caroClient.inboxId, result.addedMembers.first())
        assertEquals(
            runBlocking { group.members().map { it.inboxId }.sorted() },
            listOf(
                caroClient.inboxId,
                alixClient.inboxId,
                boClient.inboxId,
            ).sorted(),
        )
    }

    @Test
    fun testCannotStartGroupOrAddMembersWithAddressWhenExpectingInboxId() {
        assertThrows("Invalid inboxId", XMTPException::class.java) {
            runBlocking { boClient.conversations.newGroup(listOf(fixtures.alix.walletAddress)) }
        }
        val group = runBlocking { boClient.conversations.newGroup(listOf(alixClient.inboxId)) }
        assertThrows("Invalid inboxId", XMTPException::class.java) {
            runBlocking { group.addMembers(listOf(fixtures.caro.walletAddress)) }
        }
        assertThrows("Invalid inboxId", XMTPException::class.java) {
            runBlocking { group.removeMembers(listOf(fixtures.alix.walletAddress)) }
        }
    }

    @Test
    fun testCanRemoveGroupMembers() {
        val group =
            runBlocking {
                boClient.conversations.newGroup(
                    listOf(
                        alixClient.inboxId,
                        caroClient.inboxId,
                    ),
                )
            }
        runBlocking { group.removeMembers(listOf(caroClient.inboxId)) }
        assertEquals(
            runBlocking { group.members().map { it.inboxId }.sorted() },
            listOf(
                alixClient.inboxId,
                boClient.inboxId,
            ).sorted(),
        )
    }

    @Test
    fun testCanRemoveGroupMembersWhenNotCreator() {
        val boGroup =
            runBlocking {
                boClient.conversations.newGroup(
                    listOf(
                        alixClient.inboxId,
                        caroClient.inboxId,
                    ),
                )
            }
        runBlocking {
            boGroup.addAdmin(alixClient.inboxId)
            alixClient.conversations.sync()
        }
        val group =
            runBlocking {
                alixClient.conversations.sync()
                alixClient.conversations.listGroups().first()
            }
        runBlocking {
            group.removeMembers(listOf(caroClient.inboxId))
            group.sync()
            boGroup.sync()
        }
        assertEquals(
            runBlocking { boGroup.members().map { it.inboxId }.sorted() },
            listOf(
                alixClient.inboxId,
                boClient.inboxId,
            ).sorted(),
        )
    }

    @Test
    fun testCanAddGroupMemberIds() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alixClient.inboxId)) }
        val result =
            runBlocking {
                group.addMembersByIdentity(
                    listOf(
                        PublicIdentity(
                            IdentityKind.ETHEREUM,
                            fixtures.caro.walletAddress,
                        ),
                    ),
                )
            }
        assertEquals(caroClient.inboxId, result.addedMembers.first())
        assertEquals(
            runBlocking { group.members().map { it.inboxId }.sorted() },
            listOf(
                caroClient.inboxId,
                alixClient.inboxId,
                boClient.inboxId,
            ).sorted(),
        )
    }

    @Test
    fun testCanRemoveGroupMemberIds() {
        val group =
            runBlocking {
                boClient.conversations.newGroup(
                    listOf(
                        alixClient.inboxId,
                        caroClient.inboxId,
                    ),
                )
            }
        runBlocking {
            group.removeMembersByIdentity(
                listOf(
                    PublicIdentity(
                        IdentityKind.ETHEREUM,
                        fixtures.caro.walletAddress,
                    ),
                ),
            )
        }
        assertEquals(
            runBlocking { group.members().map { it.inboxId }.sorted() },
            listOf(
                alixClient.inboxId,
                boClient.inboxId,
            ).sorted(),
        )
    }

    @Test
    fun testMessageTimeIsCorrect() {
        val alixGroup = runBlocking { alixClient.conversations.newGroup(listOf(boClient.inboxId)) }
        runBlocking { alixGroup.send("Hello") }
        assertEquals(runBlocking { alixGroup.messages() }.size, 2)
        runBlocking { alixGroup.sync() }
        val message2 = runBlocking { alixGroup.messages().last() }
        runBlocking { alixGroup.sync() }
        val message3 = runBlocking { alixGroup.messages().last() }
        assertEquals(message3.id, message2.id)
        assertEquals(message3.sentAtNs, message2.sentAtNs)
    }

    @Test
    fun testIsActiveReturnsCorrectly() {
        val group =
            runBlocking {
                boClient.conversations.newGroup(
                    listOf(
                        alixClient.inboxId,
                        caroClient.inboxId,
                    ),
                )
            }
        runBlocking { caroClient.conversations.sync() }
        val caroGroup = runBlocking { caroClient.conversations.listGroups().first() }
        runBlocking { caroGroup.sync() }
        assert(runBlocking { caroGroup.isActive() })
        assert(runBlocking { group.isActive() })
        runBlocking {
            group.removeMembers(listOf(caroClient.inboxId))
            caroGroup.sync()
        }
        assert(runBlocking { group.isActive() })
        assert(!runBlocking { caroGroup.isActive() })
    }

    @Test
    fun testAddedByAddress() {
        runBlocking {
            alixClient.conversations.newGroup(
                listOf(
                    boClient.inboxId,
                ),
            )
        }
        runBlocking { boClient.conversations.sync() }
        val boGroup = runBlocking { boClient.conversations.listGroups().first() }
        assertEquals(runBlocking { boGroup.addedByInboxId() }, alixClient.inboxId)
    }

    @Test
    fun testCanListGroups() {
        runBlocking {
            boClient.conversations.newGroup(listOf(alixClient.inboxId))
            boClient.conversations.newGroup(listOf(caroClient.inboxId))
            boClient.conversations.sync()
        }
        val groups = runBlocking { boClient.conversations.listGroups() }
        assertEquals(groups.size, 2)
    }

    @Test
    fun testCanListGroupsAndConversations() {
        runBlocking {
            boClient.conversations.newGroup(listOf(alixClient.inboxId))
            boClient.conversations.newGroup(listOf(caroClient.inboxId))
            boClient.conversations.newConversation(alixClient.inboxId)
            boClient.conversations.sync()
        }
        val convos = runBlocking { boClient.conversations.list() }
        assertEquals(convos.size, 3)
    }

    @Test
    fun testCannotSendMessageToGroupMemberNotOnV3() {
        val chuxAccount = PrivateKeyBuilder()
        val chux: PrivateKey = chuxAccount.getPrivateKey()

        assertThrows(FfiException::class.java) {
            runBlocking {
                boClient.conversations.newGroupWithIdentities(
                    listOf(
                        PublicIdentity(
                            IdentityKind.ETHEREUM,
                            chux.walletAddress,
                        ),
                    ),
                )
            }
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
            val group = boClient.conversations.newGroup(listOf(alixClient.inboxId))
            group.send("howdy")
            group.send("gm")
            group.sync()
            assertEquals(group.consentState(), ConsentState.ALLOWED)
            assertEquals(
                boClient.preferences.conversationState(group.id),
                ConsentState.ALLOWED,
            )
        }
    }

    @Test
    fun testCanStreamAndUpdateNameWithoutForkingGroup() {
        val firstMsgCheck = 3
        val secondMsgCheck = 5
        var messageCallbacks = 0

        val job =
            CoroutineScope(Dispatchers.IO).launch {
                boClient.conversations.streamAllMessages().collect { _ ->
                    messageCallbacks++
                }
            }
        Thread.sleep(1000)

        val alixGroup = runBlocking { alixClient.conversations.newGroup(listOf(boClient.inboxId)) }

        runBlocking {
            alixGroup.send("hello1")
            alixGroup.updateName("hello")
            boClient.conversations.sync()
        }

        val boGroups = runBlocking { boClient.conversations.listGroups() }
        assertEquals(boGroups.size, 1)
        val boGroup = boGroups[0]
        runBlocking {
            boGroup.sync()
        }

        val boMessages1 = runBlocking { boGroup.messages() }
        assertEquals(boMessages1.size, firstMsgCheck)

        runBlocking {
            boGroup.send("hello2")
            boGroup.send("hello3")
            alixGroup.sync()
        }
        Thread.sleep(1000)
        val alixMessages = runBlocking { alixGroup.messages() }
        assertEquals(alixMessages.size, secondMsgCheck)
        runBlocking {
            alixGroup.send("hello4")
            boGroup.sync()
        }

        val boMessages2 = runBlocking { boGroup.messages() }
        assertEquals(boMessages2.size, 6)

        Thread.sleep(1000)

        assertEquals(secondMsgCheck, messageCallbacks)
        job.cancel()
    }

    @Test
    fun testsCanListGroupsFiltered() {
        runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        val group =
            runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        assertEquals(runBlocking { boClient.conversations.listGroups().size }, 2)
        assertEquals(
            runBlocking { boClient.conversations.listGroups(consentStates = listOf(ConsentState.ALLOWED)).size },
            2,
        )
        runBlocking { group.updateConsentState(ConsentState.DENIED) }
        assertEquals(
            runBlocking { boClient.conversations.listGroups(consentStates = listOf(ConsentState.ALLOWED)).size },
            1,
        )
        assertEquals(
            runBlocking { boClient.conversations.listGroups(consentStates = listOf(ConsentState.DENIED)).size },
            1,
        )
        assertEquals(
            runBlocking {
                boClient.conversations
                    .listGroups(
                        consentStates =
                            listOf(
                                ConsentState.ALLOWED,
                                ConsentState.DENIED,
                            ),
                    ).size
            },
            2,
        )
        assertEquals(runBlocking { boClient.conversations.listGroups().size }, 1)
    }

    @Test
    fun testCanListGroupsOrder() {
        val dm = runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        val group1 =
            runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        val group2 =
            runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        runBlocking { dm.send("Howdy") }
        runBlocking { group2.send("Howdy") }
        runBlocking { boClient.conversations.syncAllConversations() }
        val conversations = runBlocking { boClient.conversations.listGroups() }
        assertEquals(conversations.size, 2)
        assertEquals(conversations.map { it.id }, listOf(group2.id, group1.id))
    }

    @Test
    fun testCanSendMessageToGroup() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alixClient.inboxId)) }
        runBlocking { group.send("howdy") }
        val messageId = runBlocking { group.send("gm") }
        runBlocking { group.sync() }
        assertEquals(runBlocking { group.messages() }.first().body, "gm")
        assertEquals(runBlocking { group.messages() }.first().id, messageId)
        assertEquals(
            runBlocking { group.messages() }.first().deliveryStatus,
            MessageDeliveryStatus.PUBLISHED,
        )
        assertEquals(runBlocking { group.messages() }.size, 3)

        runBlocking { alixClient.conversations.sync() }
        val sameGroup = runBlocking { alixClient.conversations.listGroups().last() }
        runBlocking { sameGroup.sync() }
        assertEquals(runBlocking { sameGroup.messages() }.size, 3)
        assertEquals(runBlocking { sameGroup.messages() }.first().body, "gm")
    }

    @Test
    fun testCanListGroupMessages() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alixClient.inboxId)) }
        runBlocking {
            group.send("howdy")
            group.send("gm")
        }

        assertEquals(runBlocking { group.messages() }.size, 3)
        assertEquals(
            runBlocking { group.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED) }.size,
            3,
        )
        runBlocking { group.sync() }
        assertEquals(runBlocking { group.messages() }.size, 3)
        assertEquals(
            runBlocking { group.messages(deliveryStatus = MessageDeliveryStatus.UNPUBLISHED) }.size,
            0,
        )
        assertEquals(
            runBlocking { group.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED) }.size,
            3,
        )

        runBlocking { alixClient.conversations.sync() }
        val sameGroup = runBlocking { alixClient.conversations.listGroups().last() }
        runBlocking { sameGroup.sync() }
        assertEquals(
            runBlocking { sameGroup.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED) }.size,
            3,
        )
    }

    @Test
    fun testCanListGroupMessagesAfter() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alixClient.inboxId)) }
        val messageId =
            runBlocking {
                group.send("howdy")
                group.send("gm")
            }
        val message = runBlocking { boClient.conversations.findMessage(messageId) }
        assertEquals(runBlocking { group.messages() }.size, 3)
        assertEquals(runBlocking { group.messages(afterNs = message?.sentAtNs) }.size, 0)
        runBlocking {
            group.send("howdy")
            group.send("gm")
        }
        assertEquals(runBlocking { group.messages() }.size, 5)
        assertEquals(runBlocking { group.messages(afterNs = message?.sentAtNs) }.size, 2)

        runBlocking { alixClient.conversations.sync() }
        val sameGroup = runBlocking { alixClient.conversations.listGroups().last() }
        runBlocking { sameGroup.sync() }
        assertEquals(runBlocking { sameGroup.messages() }.size, 5)
        assertEquals(runBlocking { sameGroup.messages(afterNs = message?.sentAtNs) }.size, 2)
    }

    @Test
    fun testCanSendContentTypesToGroup() {
        Client.register(codec = ReactionCodec())

        val group = runBlocking { boClient.conversations.newGroup(listOf(alixClient.inboxId)) }
        runBlocking { group.send("gm") }
        runBlocking { group.sync() }
        val messageToReact = runBlocking { group.messages() }[0]

        val reaction =
            Reaction(
                reference = messageToReact.id,
                action = ReactionAction.Added,
                content = "U+1F603",
                schema = ReactionSchema.Unicode,
            )

        runBlocking {
            group.send(
                content = reaction,
                options = SendOptions(contentType = ContentTypeReaction),
            )
        }
        runBlocking { group.sync() }

        val messages = runBlocking { group.messages() }
        assertEquals(messages.size, 3)
        val content: Reaction? = messages.first().content()
        assertEquals("U+1F603", content?.content)
        assertEquals(messageToReact.id, content?.reference)
        assertEquals(ReactionAction.Added, content?.action)
        assertEquals(ReactionSchema.Unicode, content?.schema)
    }

    @Test
    fun testCanStreamGroupMessages() =
        kotlinx.coroutines.test.runTest {
            Client.register(codec = GroupUpdatedCodec())
            val membershipChange = TranscriptMessages.GroupUpdated.newBuilder().build()

            val group = boClient.conversations.newGroup(listOf(alixClient.inboxId))
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
        val group = runBlocking { caroClient.conversations.newGroup(listOf(alixClient.inboxId)) }
        val conversation =
            runBlocking { caroClient.conversations.newConversation(alixClient.inboxId) }

        runBlocking { alixClient.conversations.sync() }

        val allMessages = mutableListOf<DecodedMessage>()

        val job =
            CoroutineScope(Dispatchers.IO).launch {
                try {
                    alixClient.conversations
                        .streamAllMessages(type = ConversationFilterType.GROUPS)
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
            runBlocking { caroClient.conversations.newGroup(listOf(alixClient.inboxId)) }
        Thread.sleep(2500)

        for (i in 0 until 2) {
            runBlocking { caroGroup.send(text = "Message $i") }
            Thread.sleep(100)
        }

        assertEquals(4, allMessages.size)

        job.cancel()
    }

    @Test
    fun testCanStreamGroups() =
        kotlinx.coroutines.test.runTest {
            boClient.conversations.stream(type = ConversationFilterType.GROUPS).test {
                val group =
                    alixClient.conversations.newGroup(listOf(boClient.inboxId))
                assertEquals(group.id, awaitItem().id)
                val group2 =
                    caroClient.conversations.newGroup(listOf(boClient.inboxId))
                assertEquals(group2.id, awaitItem().id)
            }
        }

    @Test
    fun testCanStreamGroupsAndConversations() {
        val allMessages = mutableListOf<String>()

        val job =
            CoroutineScope(Dispatchers.IO).launch {
                try {
                    alixClient.conversations
                        .stream()
                        .collect { message ->
                            allMessages.add(message.topic)
                        }
                } catch (e: Exception) {
                }
            }
        Thread.sleep(2500)

        runBlocking {
            alixClient.conversations.newConversation(boClient.inboxId)
            Thread.sleep(2500)
            caroClient.conversations.newGroup(listOf(alixClient.inboxId))
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
                        alixClient.inboxId,
                        caroClient.inboxId,
                    ),
                )
            assertEquals(
                boClient.preferences.conversationState(group.id),
                ConsentState.ALLOWED,
            )
            assertEquals(group.consentState(), ConsentState.ALLOWED)

            boClient.preferences.setConsentState(
                listOf(
                    ConsentRecord(
                        group.id,
                        EntryType.CONVERSATION_ID,
                        ConsentState.DENIED,
                    ),
                ),
            )
            assertEquals(
                boClient.preferences.conversationState(group.id),
                ConsentState.DENIED,
            )
            assertEquals(group.consentState(), ConsentState.DENIED)

            group.updateConsentState(ConsentState.ALLOWED)
            assertEquals(
                boClient.preferences.conversationState(group.id),
                ConsentState.ALLOWED,
            )
            assertEquals(group.consentState(), ConsentState.ALLOWED)
        }
    }

    @Test
    fun testCanAllowAndDenyInboxId() {
        runBlocking {
            val boGroup = boClient.conversations.newGroup(listOf(alixClient.inboxId))
            assertEquals(
                boClient.preferences.inboxIdState(alixClient.inboxId),
                ConsentState.UNKNOWN,
            )
            boClient.preferences.setConsentState(
                listOf(
                    ConsentRecord(
                        alixClient.inboxId,
                        EntryType.INBOX_ID,
                        ConsentState.ALLOWED,
                    ),
                ),
            )
            var alixMember = boGroup.members().firstOrNull { it.inboxId == alixClient.inboxId }
            assertEquals(alixMember!!.consentState, ConsentState.ALLOWED)

            assertEquals(
                boClient.preferences.inboxIdState(alixClient.inboxId),
                ConsentState.ALLOWED,
            )

            boClient.preferences.setConsentState(
                listOf(
                    ConsentRecord(
                        alixClient.inboxId,
                        EntryType.INBOX_ID,
                        ConsentState.DENIED,
                    ),
                ),
            )
            alixMember = boGroup.members().firstOrNull { it.inboxId == alixClient.inboxId }
            assertEquals(alixMember!!.consentState, ConsentState.DENIED)

            assertEquals(
                boClient.preferences.inboxIdState(alixClient.inboxId),
                ConsentState.DENIED,
            )
        }
    }

    @Test
    fun testCanFetchGroupById() {
        val boGroup =
            runBlocking {
                boClient.conversations.newGroup(
                    listOf(
                        alixClient.inboxId,
                        caroClient.inboxId,
                    ),
                )
            }
        runBlocking { alixClient.conversations.sync() }
        val alixGroup = runBlocking { alixClient.conversations.findGroup(boGroup.id) }

        assertEquals(alixGroup?.id, boGroup.id)
    }

    @Test
    fun testCanFetchMessageById() {
        val boGroup =
            runBlocking {
                boClient.conversations.newGroup(
                    listOf(
                        alixClient.inboxId,
                        caroClient.inboxId,
                    ),
                )
            }
        val boMessageId = runBlocking { boGroup.send("Hello") }
        runBlocking { alixClient.conversations.sync() }
        val alixGroup = runBlocking { alixClient.conversations.findGroup(boGroup.id) }
        runBlocking { alixGroup?.sync() }
        val alixMessage = runBlocking { alixClient.conversations.findMessage(boMessageId) }

        assertEquals(alixMessage?.id, boMessageId)
    }

    @Test
    fun testUnpublishedMessages() {
        val boGroup =
            runBlocking {
                boClient.conversations.newGroup(
                    listOf(
                        alixClient.inboxId,
                        caroClient.inboxId,
                    ),
                )
            }
        runBlocking { alixClient.conversations.sync() }
        val alixGroup: Group = runBlocking { alixClient.conversations.findGroup(boGroup.id)!! }
        runBlocking { assertEquals(alixGroup.consentState(), ConsentState.UNKNOWN) }
        val preparedMessageId = runBlocking { alixGroup.prepareMessage("Test text") }
        assertEquals(runBlocking { alixGroup.messages() }.size, 2)
        assertEquals(
            runBlocking { alixGroup.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED) }.size,
            1,
        )
        assertEquals(
            runBlocking { alixGroup.messages(deliveryStatus = MessageDeliveryStatus.UNPUBLISHED) }.size,
            1,
        )

        runBlocking {
            alixGroup.publishMessages()
            alixGroup.sync()
        }
        runBlocking { assertEquals(alixGroup.consentState(), ConsentState.ALLOWED) }
        assertEquals(
            runBlocking { alixGroup.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED) }.size,
            2,
        )
        assertEquals(
            runBlocking { alixGroup.messages(deliveryStatus = MessageDeliveryStatus.UNPUBLISHED) }.size,
            0,
        )
        assertEquals(runBlocking { alixGroup.messages() }.size, 2)

        val message = runBlocking { alixGroup.messages() }.first()

        assertEquals(preparedMessageId, message.id)
    }

    @Test
    fun testSyncsAllGroupsInParallel() {
        val boGroup =
            runBlocking {
                boClient.conversations.newGroup(
                    listOf(
                        alixClient.inboxId,
                    ),
                )
            }
        val boGroup2 =
            runBlocking {
                boClient.conversations.newGroup(
                    listOf(
                        alixClient.inboxId,
                    ),
                )
            }
        runBlocking { alixClient.conversations.sync() }
        val alixGroup: Group = runBlocking { alixClient.conversations.findGroup(boGroup.id)!! }
        val alixGroup2: Group = runBlocking { alixClient.conversations.findGroup(boGroup2.id)!! }
        var syncSummary: GroupSyncSummary?

        assertEquals(runBlocking { alixGroup.messages() }.size, 1)
        assertEquals(runBlocking { alixGroup2.messages() }.size, 1)

        runBlocking {
            boGroup.send("hi")
            boGroup2.send("hi")
            syncSummary = alixClient.conversations.syncAllConversations()
        }

        assertEquals(runBlocking { alixGroup.messages() }.size, 2)
        assertEquals(runBlocking { alixGroup2.messages() }.size, 2)
        assertEquals(syncSummary?.numEligible, 3UL)

        runBlocking {
            boGroup2.removeMembers(listOf(alixClient.inboxId))
            boGroup.send("hi")
            boGroup.send("hi")
            boGroup2.send("hi")
            boGroup2.send("hi")
            syncSummary = alixClient.conversations.syncAllConversations()
            Thread.sleep(2000)
        }

        assertEquals(runBlocking { alixGroup.messages() }.size, 4)
        assertEquals(runBlocking { alixGroup2.messages() }.size, 3)
        // First syncAllGroups after remove includes the group you're removed from
        assertEquals(syncSummary?.numEligible, 3UL)

        runBlocking {
            syncSummary = alixClient.conversations.syncAllConversations()
        }
        // Next syncAllGroups will not include the inactive group
        assertEquals(syncSummary?.numSynced, 0UL)
    }

    @Test
    fun testGroupDisappearingMessages() =
        runBlocking {
            val initialSettings =
                DisappearingMessageSettings(
                    1_000_000_000,
                    1_000_000_000, // 1s duration
                )

            // Create group with disappearing messages enabled
            val boGroup =
                boClient.conversations.newGroup(
                    listOf(alixClient.inboxId),
                    disappearingMessageSettings = initialSettings,
                )
            boGroup.send("howdy")
            alixClient.conversations.syncAllConversations()

            val alixGroup = alixClient.conversations.findGroup(boGroup.id)

            // Validate messages exist and settings are applied
            assertEquals(boGroup.messages().size, 2) // memberAdd, howdy
            assertEquals(alixGroup?.messages()?.size, 2) // memberAdd, howdy
            assertNotNull(boGroup.disappearingMessageSettings())
            assertEquals(
                boGroup.disappearingMessageSettings()!!.retentionDurationInNs,
                1_000_000_000,
            )
            assertEquals(
                boGroup.disappearingMessageSettings()!!.disappearStartingAtNs,
                1_000_000_000,
            )
            Thread.sleep(5000)
            // Validate messages are deleted
            assertEquals(boGroup.messages().size, 1) // memberAdd
            assertEquals(alixGroup?.messages()?.size, 1) // memberAdd

            // Set message disappearing settings to null
            boGroup.updateDisappearingMessageSettings(null)
            boGroup.sync()
            alixGroup!!.sync()

            assertNull(boGroup.disappearingMessageSettings())
            assertNull(alixGroup.disappearingMessageSettings())
            assert(!boGroup.isDisappearingMessagesEnabled())
            assert(!alixGroup.isDisappearingMessagesEnabled())

            // Send messages after disabling disappearing settings
            boGroup.send("message after disabling disappearing")
            alixGroup.send("another message after disabling")
            boGroup.sync()

            Thread.sleep(1000)

            // Ensure messages persist
            // memberAdd, disappearing settings 1, disappearing settings 2, boMessage, alixMessage
            assertEquals(
                boGroup.messages().size,
                5,
            )
            // memberAdd disappearing settings 1, disappearing settings 2, boMessage, alixMessage
            assertEquals(
                alixGroup.messages().size,
                5,
            )

            // Re-enable disappearing messages
            val updatedSettings =
                DisappearingMessageSettings(
                    boGroup.messages().first().sentAtNs + 1_000_000_000, // 1s from now
                    1_000_000_000, // 1s duration
                )
            boGroup.updateDisappearingMessageSettings(updatedSettings)
            boGroup.sync()
            alixGroup.sync()

            Thread.sleep(2000)

            assertEquals(
                boGroup.disappearingMessageSettings()!!.disappearStartingAtNs,
                updatedSettings.disappearStartingAtNs,
            )
            assertEquals(
                alixGroup.disappearingMessageSettings()!!.disappearStartingAtNs,
                updatedSettings.disappearStartingAtNs,
            )

            // Send new messages
            boGroup.send("this will disappear soon")
            alixGroup.send("so will this")
            boGroup.sync()

            // memberAdd, disappearing settings 3, disappearing settings 4, boMessage, alixMessage,
            // disappearing settings 5, disappearing settings 6, boMessage2, alixMessage2
            assertEquals(
                boGroup.messages().size,
                9,
            )
            // memberAdd disappearing settings 3, disappearing settings 4, boMessage, alixMessage,
            // disappearing settings 5, disappearing settings 6, boMessage2, alixMessage2
            assertEquals(
                alixGroup.messages().size,
                9,
            )

            Thread.sleep(6000) // Wait for messages to disappear

            // Validate messages were deleted
            // memberAdd, disappearing settings 3, disappearing settings 4, boMessage, alixMessage,
            // disappearing settings 5, disappearing settings 6
            assertEquals(
                boGroup.messages().size,
                7,
            )
            // memberAdd disappearing settings 3, disappearing settings 4, boMessage, alixMessage,
            // disappearing settings 5, disappearing settings 6
            assertEquals(
                alixGroup.messages().size,
                7,
            )

            // Final validation that settings persist
            assertEquals(
                boGroup.disappearingMessageSettings()!!.retentionDurationInNs,
                updatedSettings.retentionDurationInNs,
            )
            assertEquals(
                alixGroup.disappearingMessageSettings()!!.retentionDurationInNs,
                updatedSettings.retentionDurationInNs,
            )
            assert(boGroup.isDisappearingMessagesEnabled())
            assert(alixGroup.isDisappearingMessagesEnabled())
        }

    @Test
    fun testGroupPausedForVersionReturnsNone() =
        runBlocking {
            val boGroup =
                boClient.conversations.newGroup(
                    listOf(alixClient.inboxId),
                )
            val pausedForVersionGroup = boGroup.pausedForVersion()
            assertNull(pausedForVersionGroup)

            val boDm = boClient.conversations.newConversation(alixClient.inboxId)
            val pausedForVersionDm = boDm.pausedForVersion()
            assertNull(pausedForVersionDm)
        }

    @Test
    fun testCanQueryMessagesByInsertedTime() {
        runBlocking {
            val group = boClient.conversations.newGroup(listOf(alixClient.inboxId))
            group.send("first")
            group.send("second")
            group.sync()

            val messages = group.messages()
            assertEquals(3, messages.size)

            // Verify insertedAtNs is populated
            val firstMessage = messages.last()
            assert(firstMessage.insertedAtNs > 0)

            // Test insertedAfterNs filter
            val filteredMessages = group.messages(insertedAfterNs = firstMessage.insertedAtNs)
            assertEquals(2, filteredMessages.size)

            // Test sortBy parameter
            val sortedBySent = group.messages(sortBy = SortBy.SENT_TIME)
            val sortedByInserted = group.messages(sortBy = SortBy.INSERTED_TIME)
            assertEquals(sortedBySent.size, sortedByInserted.size)

            // Test countMessages with insertedAfterNs
            val count = group.countMessages(insertedAfterNs = firstMessage.insertedAtNs)
            assertEquals(2, count)
        }
    }

    @Test
    fun testCountMessagesWithExcludedContentTypes() {
        Client.register(codec = ReactionCodec())

        val group = runBlocking { boClient.conversations.newGroup(listOf(alixClient.inboxId)) }
        runBlocking {
            group.send("gm")
            group.sync()
        }
        val messageToReact = runBlocking { group.messages() }[0]

        val reaction =
            Reaction(
                reference = messageToReact.id,
                action = ReactionAction.Added,
                content = "U+1F603",
                schema = ReactionSchema.Unicode,
            )

        runBlocking {
            group.send(
                content = reaction,
                options = SendOptions(contentType = ContentTypeReaction),
            )
        }

        // Count without exclusions - should include memberAdd, text message, and reaction
        val totalCount = runBlocking { group.countMessages() }
        assertEquals(3, totalCount)

        // Count with reaction exclusion - should only include memberAdd and text message
        val countWithoutReactions =
            runBlocking {
                group.countMessages(
                    excludeContentTypes =
                        listOf(
                            uniffi.xmtpv3.FfiContentType.REACTION,
                        ),
                )
            }
        assertEquals(2, countWithoutReactions)
    }

    @Test
    fun testCanLeaveGroup() =
        runBlocking {
            // Create group with alix and bo and verify we have 2 members and group is active for Alix
            val boGroup = boClient.conversations.newGroup(listOf(alixClient.inboxId))
            alixClient.conversations.syncAllConversations()
            boClient.conversations.syncAllConversations()
            val alixGroup = alixClient.conversations.findGroup(boGroup.id)
            assertNotNull(alixGroup)
            val groupMembers = boGroup.members()
            assertEquals(2, groupMembers.size)
            assert(alixGroup!!.isActive())

            // Alix leaves group and bo syncs
            alixGroup.leaveGroup()
            alixGroup.sync()
            boGroup.sync()
            // Alix Group is still active until worker runs
            assert(alixGroup.isActive())

            // Delay here so that the removal is processed
            // Verify 1 member and group is no longer active
            Thread.sleep(3000) // 3 seconds
            val groupMembersAfterLeave = boGroup.members()
            assertEquals(1, groupMembersAfterLeave.size)
            alixGroup.sync()
            assert(!alixGroup.isActive())
        }

    @Test
    fun testSelfRemovalWithMembershipState() =
        runBlocking {
            // Alix creates a group and adds Bo
            val alixGroup =
                alixClient.conversations.newGroup(
                    listOf(boClient.inboxId),
                )

            // Bo syncs and gets the group
            boClient.conversations.sync()
            val boGroup = boClient.conversations.findGroup(alixGroup.id)
            assertNotNull("Bo should have received the group", boGroup)

            // Verify Bo's membership state is PENDING when first invited
            val boStateInitial = boGroup!!.membershipState()
            assertEquals(
                "Bo should be in PENDING state when first invited",
                GroupMembershipState.PENDING,
                boStateInitial,
            )

            // Verify Alix's membership state is ALLOWED (creator)
            val alixState = alixGroup.membershipState()
            assertEquals(
                "Alix should be in ALLOWED state as group creator",
                GroupMembershipState.ALLOWED,
                alixState,
            )

            // Bo leaves the group
            boGroup.leaveGroup()

            // Verify Bo's membership state is PENDING_REMOVE after requesting to leave
            val boStateAfterLeave = boGroup.membershipState()
            assertEquals(
                "Bo should be in PENDING_REMOVE state after leaving",
                GroupMembershipState.PENDING_REMOVE,
                boStateAfterLeave,
            )

            // Alix syncs to process the leave request
            alixGroup.sync()

            // Wait for admin worker to process the removal
            Thread.sleep(2000)

            // Bo syncs to get the final removal
            boGroup.sync()

            // Verify Bo's group is no longer active
            assert(!boGroup.isActive())

            // Verify Alix's membership state remains ALLOWED
            val alixStateFinal = alixGroup.membershipState()
            assertEquals(
                "Alix should remain in ALLOWED state",
                GroupMembershipState.ALLOWED,
                alixStateFinal,
            )

            // Verify only Alix remains in the group
            alixGroup.sync()
            val members = alixGroup.members()
            assertEquals("Only Alix should remain in the group", 1, members.size)
        }
}
