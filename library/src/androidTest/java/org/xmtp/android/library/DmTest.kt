package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import app.cash.turbine.test
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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
import org.xmtp.android.library.libxmtp.DisappearingMessageSettings
import org.xmtp.android.library.libxmtp.IdentityKind
import org.xmtp.android.library.libxmtp.Message
import org.xmtp.android.library.libxmtp.Message.MessageDeliveryStatus
import org.xmtp.android.library.libxmtp.PublicIdentity
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress
import uniffi.xmtpv3.GenericException

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
            val convo1 = boClient.conversations.findOrCreateDm(alixClient.inboxId)
            alixClient.conversations.sync()
            val sameConvo1 = alixClient.conversations.findOrCreateDm(boClient.inboxId)
            assertEquals(convo1.id, sameConvo1.id)
        }
    }

    @Test
    fun testCanCreateADmWithInboxId() {
        runBlocking {
            val convo1 = boClient.conversations.findOrCreateDmWithIdentity(
                PublicIdentity(
                    IdentityKind.ETHEREUM,
                    alix.walletAddress
                )
            )
            alixClient.conversations.sync()
            val sameConvo1 = alixClient.conversations.findOrCreateDmWithIdentity(
                PublicIdentity(
                    IdentityKind.ETHEREUM,
                    bo.walletAddress
                )
            )
            assertEquals(convo1.id, sameConvo1.id)
        }
    }

    @Test
    fun testsCanFindDmByInboxId() {
        runBlocking {
            val dm = boClient.conversations.findOrCreateDm(caroClient.inboxId)

            val caroDm = boClient.conversations.findDmByInboxId(caroClient.inboxId)
            val alixDm = boClient.conversations.findDmByInboxId(alixClient.inboxId)
            assertNull(alixDm)
            assertEquals(caroDm?.id, dm.id)
        }
    }

    @Test
    fun testsCanFindDmByIdentity() {
        runBlocking {
            val dm = boClient.conversations.findOrCreateDm(caroClient.inboxId)

            val caroDm = boClient.conversations.findDmByIdentity(
                PublicIdentity(
                    IdentityKind.ETHEREUM,
                    caro.walletAddress
                )
            )
            val alixDm = boClient.conversations.findDmByIdentity(
                PublicIdentity(
                    IdentityKind.ETHEREUM,
                    alix.walletAddress
                )
            )
            assertNull(alixDm)
            assertEquals(caroDm?.id, dm.id)
        }
    }

    @Test
    fun testCanListDmMembers() {
        val dm = runBlocking {
            boClient.conversations.findOrCreateDm(
                alixClient.inboxId,
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

        assertThrows(GenericException::class.java) {
            runBlocking { boClient.conversations.findOrCreateDmWithIdentity(PublicIdentity(IdentityKind.ETHEREUM, chux.walletAddress)) }
        }
    }

    @Test
    fun testCannotStartDmWithSelf() {
        assertThrows("Recipient is sender", XMTPException::class.java) {
            runBlocking { boClient.conversations.findOrCreateDm(boClient.inboxId) }
        }
    }

    @Test
    fun testDmStartsWithAllowedState() {
        runBlocking {
            val dm = boClient.conversations.findOrCreateDm(alixClient.inboxId)
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
        runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        val dm = runBlocking { boClient.conversations.findOrCreateDm(alixClient.inboxId) }
        assertEquals(runBlocking { boClient.conversations.listDms().size }, 2)
        assertEquals(
            runBlocking { boClient.conversations.listDms(consentStates = listOf(ConsentState.ALLOWED)).size },
            2
        )
        runBlocking { dm.updateConsentState(ConsentState.DENIED) }
        assertEquals(
            runBlocking { boClient.conversations.listDms(consentStates = listOf(ConsentState.ALLOWED)).size },
            1
        )
        assertEquals(
            runBlocking { boClient.conversations.listDms(consentStates = listOf(ConsentState.DENIED)).size },
            1
        )
        assertEquals(
            runBlocking {
                boClient.conversations.listDms(
                    consentStates = listOf(
                        ConsentState.ALLOWED,
                        ConsentState.DENIED
                    )
                ).size
            },
            2
        )
        assertEquals(runBlocking { boClient.conversations.listDms().size }, 2)
    }

    @Test
    fun testCanListDmsOrder() {
        val dm1 = runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        val dm2 =
            runBlocking { boClient.conversations.findOrCreateDm(alixClient.inboxId) }
        val group =
            runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        runBlocking { dm2.send("Howdy") }
        runBlocking { group.send("Howdy") }
        runBlocking { boClient.conversations.syncAllConversations() }
        val conversations = runBlocking { boClient.conversations.listDms() }
        assertEquals(conversations.size, 2)
        assertEquals(conversations.map { it.id }, listOf(dm2.id, dm1.id))
    }

    @Test
    fun testCanSendMessageToDm() {
        val dm = runBlocking { boClient.conversations.findOrCreateDm(alixClient.inboxId) }
        runBlocking { dm.send("howdy") }
        val messageId = runBlocking { dm.send("gm") }
        runBlocking { dm.sync() }
        assertEquals(runBlocking { dm.messages() }.first().body, "gm")
        assertEquals(runBlocking { dm.messages() }.first().id, messageId)
        assertEquals(
            runBlocking { dm.messages() }.first().deliveryStatus,
            MessageDeliveryStatus.PUBLISHED
        )
        assertEquals(runBlocking { dm.messages() }.size, 3)

        runBlocking { alixClient.conversations.sync() }
        val sameDm = runBlocking { alixClient.conversations.listDms().last() }
        runBlocking { sameDm.sync() }
        assertEquals(runBlocking { sameDm.messages() }.size, 2)
        assertEquals(runBlocking { sameDm.messages() }.first().body, "gm")
    }

    @Test
    fun testCanListDmMessages() {
        val dm = runBlocking { boClient.conversations.findOrCreateDm(alixClient.inboxId) }
        runBlocking {
            dm.send("howdy")
            dm.send("gm")
        }

        assertEquals(runBlocking { dm.messages() }.size, 3)
        assertEquals(
            runBlocking { dm.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED) }.size,
            3
        )
        runBlocking { dm.sync() }
        assertEquals(runBlocking { dm.messages() }.size, 3)
        assertEquals(
            runBlocking { dm.messages(deliveryStatus = MessageDeliveryStatus.UNPUBLISHED) }.size,
            0
        )
        assertEquals(
            runBlocking { dm.messages(deliveryStatus = MessageDeliveryStatus.PUBLISHED) }.size,
            3
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

        val dm = runBlocking { boClient.conversations.findOrCreateDm(alixClient.inboxId) }
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
        assertEquals(messages.size, 3)
        val content: Reaction? = messages.first().content()
        assertEquals("U+1F603", content?.content)
        assertEquals(messageToReact.id, content?.reference)
        assertEquals(ReactionAction.Added, content?.action)
        assertEquals(ReactionSchema.Unicode, content?.schema)
    }

    @Test
    fun testCanStreamDmMessages() = kotlinx.coroutines.test.runTest {
        val group = boClient.conversations.findOrCreateDm(alixClient.inboxId)
        alixClient.conversations.sync()
        val alixDm = alixClient.conversations.findDmByIdentity(
            PublicIdentity(
                IdentityKind.ETHEREUM,
                bo.walletAddress
            )
        )
        group.streamMessages().test {
            alixDm?.send("hi")
            assertEquals("hi", awaitItem().body)
            alixDm?.send("hi again")
            assertEquals("hi again", awaitItem().body)
        }
    }

    @Test
    fun testCanStreamAllMessages() {
        val boDm = runBlocking { boClient.conversations.findOrCreateDm(alixClient.inboxId) }
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
            runBlocking { caroClient.conversations.findOrCreateDm(alixClient.inboxId) }
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
                alixClient.conversations.findOrCreateDm(boClient.inboxId)
            assertEquals(dm.id, awaitItem().id)
            val dm2 =
                caroClient.conversations.findOrCreateDm(boClient.inboxId)
            assertEquals(dm2.id, awaitItem().id)
        }
    }

    @Test
    fun testDmConsent() {
        runBlocking {
            val dm =
                boClient.conversations.findOrCreateDm(alixClient.inboxId)
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

    @Test
    fun testDmDisappearingMessages() = runBlocking {
        val initialSettings = DisappearingMessageSettings(
            1_000_000_000,
            1_000_000_000 // 1s duration
        )

        // Create group with disappearing messages enabled
        val boDm = boClient.conversations.findOrCreateDm(
            alixClient.inboxId,
            disappearingMessageSettings = initialSettings
        )
        boDm.send("howdy")
        alixClient.conversations.syncAllConversations()

        val alixDm = alixClient.conversations.findDmByInboxId(boClient.inboxId)

        // Validate messages exist and settings are applied
        assertEquals(boDm.messages().size, 2) // memberAdd howdy
        assertEquals(alixDm?.messages()?.size, 1) // howdy
        Assert.assertNotNull(boDm.disappearingMessageSettings)
        assertEquals(boDm.disappearingMessageSettings!!.retentionDurationInNs, 1_000_000_000)
        assertEquals(boDm.disappearingMessageSettings!!.disappearStartingAtNs, 1_000_000_000)
        Thread.sleep(5000)
        // Validate messages are deleted
        assertEquals(boDm.messages().size, 1)
        assertEquals(alixDm?.messages()?.size, 0)

        // Set message disappearing settings to null
        boDm.updateDisappearingMessageSettings(null)
        boDm.sync()
        alixDm!!.sync()

        assertNull(boDm.disappearingMessageSettings)
        assertNull(alixDm.disappearingMessageSettings)
        assertFalse(boDm.isDisappearingMessagesEnabled)
        assertFalse(alixDm.isDisappearingMessagesEnabled)

        // Send messages after disabling disappearing settings
        boDm.send("message after disabling disappearing")
        alixDm.send("another message after disabling")
        boDm.sync()

        Thread.sleep(1000)

        // Ensure messages persist
        assertEquals(
            boDm.messages().size,
            5
        ) // memberAss disappearing settings 1, disappearing settings 2, boMessage, alixMessage
        assertEquals(
            alixDm.messages().size,
            4
        ) // disappearing settings 1, disappearing settings 2, boMessage, alixMessage

        // Re-enable disappearing messages
        val updatedSettings = DisappearingMessageSettings(
            boDm.messages().first().sentAtNs + 1_000_000_000, // 1s from now
            1_000_000_000 // 1s duration
        )
        boDm.updateDisappearingMessageSettings(updatedSettings)
        boDm.sync()
        alixDm.sync()

        Thread.sleep(1000)

        assertEquals(
            boDm.disappearingMessageSettings!!.disappearStartingAtNs,
            updatedSettings.disappearStartingAtNs
        )
        assertEquals(
            alixDm.disappearingMessageSettings!!.disappearStartingAtNs,
            updatedSettings.disappearStartingAtNs
        )

        // Send new messages
        boDm.send("this will disappear soon")
        alixDm.send("so will this")
        boDm.sync()

        assertEquals(
            boDm.messages().size,
            9
        ) // memberAdd disappearing settings 3, disappearing settings 4, boMessage, alixMessage, disappearing settings 5, disappearing settings 6, boMessage2, alixMessage2
        assertEquals(
            alixDm.messages().size,
            8
        ) // disappearing settings 3, disappearing settings 4, boMessage, alixMessage, disappearing settings 5, disappearing settings 6, boMessage2, alixMessage2

        Thread.sleep(6000) // Wait for messages to disappear

        // Validate messages were deleted
        assertEquals(
            boDm.messages().size,
            7
        ) // memberAdd disappearing settings 3, disappearing settings 4, boMessage, alixMessage, disappearing settings 5, disappearing settings 6
        assertEquals(
            alixDm.messages().size,
            6
        ) // disappearing settings 3, disappearing settings 4, boMessage, alixMessage, disappearing settings 5, disappearing settings 6

        // Final validation that settings persist
        assertEquals(
            boDm.disappearingMessageSettings!!.retentionDurationInNs,
            updatedSettings.retentionDurationInNs
        )
        assertEquals(
            alixDm.disappearingMessageSettings!!.retentionDurationInNs,
            updatedSettings.retentionDurationInNs
        )
        assert(boDm.isDisappearingMessagesEnabled)
        assert(alixDm.isDisappearingMessagesEnabled)
    }
}
