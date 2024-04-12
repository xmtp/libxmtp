package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import app.cash.turbine.test
import com.google.protobuf.kotlin.toByteString
import com.google.protobuf.kotlin.toByteStringUtf8
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Ignore
import org.junit.Test
import org.junit.runner.RunWith
import org.web3j.crypto.Hash
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.InvitationV1
import org.xmtp.android.library.messages.InvitationV1ContextBuilder
import org.xmtp.android.library.messages.MessageBuilder
import org.xmtp.android.library.messages.MessageHeaderV2Builder
import org.xmtp.android.library.messages.MessageV1Builder
import org.xmtp.android.library.messages.MessageV2Builder
import org.xmtp.android.library.messages.Pagination
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.SealedInvitationBuilder
import org.xmtp.android.library.messages.SealedInvitationHeaderV1
import org.xmtp.android.library.messages.SignedContentBuilder
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.createDeterministic
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.header
import org.xmtp.android.library.messages.recoverWalletSignerPublicKey
import org.xmtp.android.library.messages.sign
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.toSignedPublicKeyBundle
import org.xmtp.android.library.messages.toV2
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import org.xmtp.proto.message.contents.Invitation
import org.xmtp.proto.message.contents.Invitation.InvitationV1.Context
import java.nio.charset.StandardCharsets
import java.util.Date

@RunWith(AndroidJUnit4::class)
class ConversationTest {
    lateinit var fakeApiClient: FakeApiClient
    lateinit var aliceWallet: PrivateKeyBuilder
    lateinit var bobWallet: PrivateKeyBuilder
    lateinit var alice: PrivateKey
    lateinit var aliceClient: Client
    lateinit var bob: PrivateKey
    lateinit var bobClient: Client
    lateinit var fixtures: Fixtures

    @Before
    fun setUp() {
        fixtures = fixtures()
        aliceWallet = fixtures.aliceAccount
        alice = fixtures.alice
        bobWallet = fixtures.bobAccount
        bob = fixtures.bob
        fakeApiClient = fixtures.fakeApiClient
        aliceClient = fixtures.aliceClient
        bobClient = fixtures.bobClient
    }

    @Test
    fun testDoesNotAllowConversationWithSelf() {
        val client = Client().create(account = aliceWallet)
        assertThrows("Recipient is sender", XMTPException::class.java) {
            runBlocking { client.conversations.newConversation(alice.walletAddress) }
        }
    }

    @Test
    fun testCanFindExistingV1Conversation() {
        val encoder = TextCodec()
        val encodedContent = encoder.encode(content = "hi alice")
        // Get a date that's roughly two weeks ago to test with
        val someTimeAgo = Date(System.currentTimeMillis() - 2_000_000)
        val messageV1 = MessageV1Builder.buildEncode(
            sender = bobClient.privateKeyBundleV1!!,
            recipient = aliceClient.privateKeyBundleV1?.toPublicKeyBundle()!!,
            message = encodedContent.toByteArray(),
            timestamp = someTimeAgo,
        )
        // Overwrite contact as legacy
        runBlocking {
            bobClient.publishUserContact(legacy = true)
            aliceClient.publishUserContact(legacy = true)
        }
        runBlocking {
            bobClient.publish(
                envelopes = listOf(
                    EnvelopeBuilder.buildFromTopic(
                        topic = Topic.userIntro(bob.walletAddress),
                        timestamp = someTimeAgo,
                        message = MessageBuilder.buildFromMessageV1(v1 = messageV1).toByteArray(),
                    ),
                    EnvelopeBuilder.buildFromTopic(
                        topic = Topic.userIntro(alice.walletAddress),
                        timestamp = someTimeAgo,
                        message = MessageBuilder.buildFromMessageV1(v1 = messageV1).toByteArray(),
                    ),
                    EnvelopeBuilder.buildFromTopic(
                        topic = Topic.directMessageV1(
                            bob.walletAddress,
                            alice.walletAddress,
                        ),
                        timestamp = someTimeAgo,
                        message = MessageBuilder.buildFromMessageV1(v1 = messageV1).toByteArray(),
                    ),
                ),
            )
        }
        var conversation =
            runBlocking { aliceClient.conversations.newConversation(bob.walletAddress) }
        assertEquals(conversation.peerAddress, bob.walletAddress)
        assertEquals(conversation.createdAt, someTimeAgo)
        val existingMessages = fakeApiClient.published.size
        conversation = runBlocking { bobClient.conversations.newConversation(alice.walletAddress) }

        assertEquals(
            "published more messages when we shouldn't have",
            existingMessages,
            fakeApiClient.published.size,
        )
        assertEquals(conversation.peerAddress, alice.walletAddress)
        assertEquals(conversation.createdAt, someTimeAgo)
    }

    @Test
    fun testCanFindExistingV2Conversation() {
        val existingConversation = runBlocking {
            bobClient.conversations.newConversation(
                alice.walletAddress,
                context = InvitationV1ContextBuilder.buildFromConversation("http://example.com/2"),
            )
        }
        var conversation: Conversation? = null
        fakeApiClient.assertNoPublish {
            runBlocking {
                conversation = bobClient.conversations.newConversation(
                    alice.walletAddress,
                    context = InvitationV1ContextBuilder.buildFromConversation("http://example.com/2"),
                )
            }
        }
        assertEquals(
            "made new conversation instead of using existing one",
            conversation!!.topic,
            existingConversation.topic,
        )
    }

    @Test
    fun testCanLoadV1Messages() {
        // Overwrite contact as legacy so we can get v1
        fixtures.publishLegacyContact(client = bobClient)
        fixtures.publishLegacyContact(client = aliceClient)
        val bobConversation =
            runBlocking { bobClient.conversations.newConversation(aliceWallet.address) }
        val aliceConversation =
            runBlocking { aliceClient.conversations.newConversation(bobWallet.address) }

        runBlocking { bobConversation.send(content = "hey alice") }
        runBlocking { bobConversation.send(content = "hey alice again") }
        val messages = runBlocking { aliceConversation.messages() }
        assertEquals(2, messages.size)
        assertEquals("hey alice", messages[1].body)
        assertEquals(bobWallet.address, messages[1].senderAddress)
    }

    @Test
    fun testCanLoadV2Messages() {
        val bobConversation = runBlocking {
            bobClient.conversations.newConversation(
                aliceWallet.address,
                InvitationV1ContextBuilder.buildFromConversation("hi"),
            )
        }

        val aliceConversation = runBlocking {
            aliceClient.conversations.newConversation(
                bobWallet.address,
                InvitationV1ContextBuilder.buildFromConversation("hi"),
            )
        }
        runBlocking { bobConversation.send(content = "hey alice") }
        val messages = runBlocking { aliceConversation.messages() }
        assertEquals(1, messages.size)
        assertEquals("hey alice", messages[0].body)
        assertEquals(bobWallet.address, messages[0].senderAddress)
    }

    @Test
    fun testVerifiesV2MessageSignature() {
        val aliceConversation = runBlocking {
            aliceClient.conversations.newConversation(
                bobWallet.address,
                context = InvitationV1ContextBuilder.buildFromConversation(conversationId = "hi"),
            )
        }

        val codec = TextCodec()
        val originalContent = codec.encode(content = "hello")
        val tamperedContent = codec.encode(content = "this is a fake")
        val originalPayload = originalContent.toByteArray()
        val tamperedPayload = tamperedContent.toByteArray()
        val date = Date()
        val header = MessageHeaderV2Builder.buildFromTopic(aliceConversation.topic, created = date)
        val headerBytes = header.toByteArray()
        val digest = Hash.sha256(headerBytes + tamperedPayload)
        val preKey = aliceClient.keys?.preKeysList?.get(0)
        val signature = preKey?.sign(digest)
        val bundle = aliceClient.privateKeyBundleV1?.toV2()?.getPublicKeyBundle()
        val signedContent = SignedContentBuilder.builderFromPayload(
            payload = originalPayload,
            sender = bundle,
            signature = signature,
        )
        val signedBytes = signedContent.toByteArray()
        val ciphertext = Crypto.encrypt(
            aliceConversation.keyMaterial!!,
            signedBytes,
            additionalData = headerBytes,
        )
        val thirtyDayPeriodsSinceEpoch =
            (Date().time / 1000 / 60 / 60 / 24 / 30).toInt()
        val info = "$thirtyDayPeriodsSinceEpoch-${aliceClient.address}"
        val infoEncoded = info.toByteStringUtf8().toByteArray()
        val senderHmacGenerated =
            Crypto.calculateMac(
                Crypto.deriveKey(aliceConversation.keyMaterial!!, ByteArray(0), infoEncoded),
                headerBytes
            )
        val tamperedMessage =
            MessageV2Builder.buildFromCipherText(
                headerBytes = headerBytes,
                ciphertext = ciphertext,
                senderHmac = senderHmacGenerated,
                shouldPush = codec.shouldPush("this is a fake"),
            )
        val tamperedEnvelope = EnvelopeBuilder.buildFromString(
            topic = aliceConversation.topic,
            timestamp = Date(),
            message = MessageBuilder.buildFromMessageV2(v2 = tamperedMessage.messageV2)
                .toByteArray(),
        )
        runBlocking { aliceClient.publish(envelopes = listOf(tamperedEnvelope)) }
        val bobConversation = runBlocking {
            bobClient.conversations.newConversation(
                aliceWallet.address,
                InvitationV1ContextBuilder.buildFromConversation("hi"),
            )
        }
        assertThrows("Invalid signature", XMTPException::class.java) {
            bobConversation.decode(tamperedEnvelope)
        }
        // But it should be properly discarded from the message listing.
        runBlocking {
            assertEquals(0, bobConversation.messages().size)
        }
    }

    @Test
    fun testCanSendGzipCompressedV1Messages() {
        fixtures.publishLegacyContact(client = bobClient)
        fixtures.publishLegacyContact(client = aliceClient)
        val bobConversation =
            runBlocking { bobClient.conversations.newConversation(aliceWallet.address) }
        val aliceConversation =
            runBlocking { aliceClient.conversations.newConversation(bobWallet.address) }
        runBlocking {
            bobConversation.send(
                text = MutableList(1000) { "A" }.toString(),
                sendOptions = SendOptions(compression = EncodedContentCompression.GZIP),
            )
        }
        val messages = runBlocking { aliceConversation.messages() }
        assertEquals(1, messages.size)
        assertEquals(MutableList(1000) { "A" }.toString(), messages[0].content())
    }

    @Test
    fun testCanSendDeflateCompressedV1Messages() {
        fixtures.publishLegacyContact(client = bobClient)
        fixtures.publishLegacyContact(client = aliceClient)
        val bobConversation =
            runBlocking { bobClient.conversations.newConversation(aliceWallet.address) }
        val aliceConversation =
            runBlocking { aliceClient.conversations.newConversation(bobWallet.address) }
        runBlocking {
            bobConversation.send(
                content = MutableList(1000) { "A" }.toString(),
                options = SendOptions(compression = EncodedContentCompression.DEFLATE),
            )
        }
        val messages = runBlocking { aliceConversation.messages() }
        assertEquals(1, messages.size)
        assertEquals(MutableList(1000) { "A" }.toString(), messages[0].content())
    }

    @Test
    fun testCanSendGzipCompressedV2Messages() {
        val bobConversation = runBlocking {
            bobClient.conversations.newConversation(
                aliceWallet.address,
                InvitationV1ContextBuilder.buildFromConversation(conversationId = "hi"),
            )
        }
        val aliceConversation = runBlocking {
            aliceClient.conversations.newConversation(
                bobWallet.address,
                InvitationV1ContextBuilder.buildFromConversation(conversationId = "hi"),
            )
        }
        runBlocking {
            bobConversation.send(
                text = MutableList(1000) { "A" }.toString(),
                sendOptions = SendOptions(compression = EncodedContentCompression.GZIP),
            )
        }
        val messages = runBlocking { aliceConversation.messages() }
        assertEquals(1, messages.size)
        assertEquals(MutableList(1000) { "A" }.toString(), messages[0].body)
        assertEquals(bobWallet.address, messages[0].senderAddress)
    }

    @Test
    fun testCanSendDeflateCompressedV2Messages() {
        val bobConversation = runBlocking {
            bobClient.conversations.newConversation(
                aliceWallet.address,
                InvitationV1ContextBuilder.buildFromConversation(conversationId = "hi"),
            )
        }
        val aliceConversation = runBlocking {
            aliceClient.conversations.newConversation(
                bobWallet.address,
                InvitationV1ContextBuilder.buildFromConversation(conversationId = "hi"),
            )
        }
        runBlocking {
            bobConversation.send(
                content = MutableList(1000) { "A" }.toString(),
                options = SendOptions(compression = EncodedContentCompression.DEFLATE),
            )
        }
        val messages = runBlocking { aliceConversation.messages() }
        assertEquals(1, messages.size)
        assertEquals(MutableList(1000) { "A" }.toString(), messages[0].body)
        assertEquals(bobWallet.address, messages[0].senderAddress)
    }

    @Test
    fun testEndToEndConversation() {
        val fakeContactWallet = PrivateKeyBuilder()
        val fakeContactClient = Client().create(account = fakeContactWallet)
        runBlocking { fakeContactClient.publishUserContact() }
        val fakeWallet = PrivateKeyBuilder()
        val client = Client().create(account = fakeWallet)
        val contact = client.getUserContact(peerAddress = fakeContactWallet.address)!!
        assertEquals(contact.walletAddress, fakeContactWallet.address)
        val created = Date()
        val invitationContext = Invitation.InvitationV1.Context.newBuilder().also {
            it.conversationId = "https://example.com/1"
        }.build()
        val invitationv1 = InvitationV1.newBuilder().build().createDeterministic(
            sender = client.keys,
            recipient = fakeContactClient.keys.getPublicKeyBundle(),
            context = invitationContext,
        )
        val senderBundle = client.privateKeyBundleV1?.toV2()
        assertEquals(
            senderBundle?.identityKey?.publicKey?.recoverWalletSignerPublicKey()?.walletAddress,
            fakeWallet.address,
        )
        val invitation = SealedInvitationBuilder.buildFromV1(
            sender = client.privateKeyBundleV1!!.toV2(),
            recipient = contact.toSignedPublicKeyBundle(),
            created = created,
            invitation = invitationv1,
        )
        val inviteHeader = invitation.v1.header
        assertEquals(inviteHeader.sender.walletAddress, fakeWallet.address)
        assertEquals(inviteHeader.recipient.walletAddress, fakeContactWallet.address)
        val header = SealedInvitationHeaderV1.parseFrom(invitation.v1.headerBytes)
        val conversation =
            ConversationV2.create(client = client, invitation = invitationv1, header = header)
        assertEquals(fakeContactWallet.address, conversation.peerAddress)

        runBlocking { conversation.send(content = "hello world") }

        val conversationList = runBlocking { client.conversations.list() }
        val recipientConversation = conversationList.lastOrNull()

        val messages = runBlocking { recipientConversation?.messages() }
        val message = messages?.firstOrNull()
        if (message != null) {
            assertEquals("hello world", message.body)
        }
    }

    @Test
    fun testCanUseCachedConversation() {
        runBlocking { bobClient.conversations.newConversation(alice.walletAddress) }

        fakeApiClient.assertNoQuery {
            runBlocking { bobClient.conversations.newConversation(alice.walletAddress) }
        }
    }

    @Test
    @Ignore("Rust seems to be Flaky with V1")
    fun testCanPaginateV1Messages() {
        // Overwrite contact as legacy so we can get v1
        fixtures.publishLegacyContact(client = bobClient)
        fixtures.publishLegacyContact(client = aliceClient)
        val bobConversation =
            runBlocking { bobClient.conversations.newConversation(alice.walletAddress) }
        val aliceConversation =
            runBlocking { aliceClient.conversations.newConversation(bob.walletAddress) }

        val date = Date()
        date.time = date.time - 1000000
        runBlocking { bobConversation.send(text = "hey alice 1", sentAt = date) }
        runBlocking { bobConversation.send(text = "hey alice 2") }
        runBlocking { bobConversation.send(text = "hey alice 3") }
        val messages = runBlocking { aliceConversation.messages(limit = 1) }
        assertEquals(1, messages.size)
        assertEquals("hey alice 3", messages[0].body)
    }

    @Test
    fun testCanPaginateV2Messages() {
        val bobConversation = runBlocking {
            bobClient.conversations.newConversation(
                alice.walletAddress,
                context = InvitationV1ContextBuilder.buildFromConversation("hi"),
            )
        }
        val aliceConversation = runBlocking {
            aliceClient.conversations.newConversation(
                bob.walletAddress,
                context = InvitationV1ContextBuilder.buildFromConversation("hi"),
            )
        }
        val date = Date()
        date.time = date.time - 1000000
        runBlocking {
            bobConversation.send(text = "hey alice 1", sentAt = date)
            bobConversation.send(text = "hey alice 2")
            bobConversation.send(text = "hey alice 3")
            val messages = aliceConversation.messages(limit = 1)
            assertEquals(1, messages.size)
            assertEquals("hey alice 3", messages[0].body)
            val messages2 = aliceConversation.messages(limit = 1, after = date)
            assertEquals(1, messages2.size)
            assertEquals("hey alice 3", messages2[0].body)
            val messagesAsc =
                aliceConversation.messages(direction = MessageApiOuterClass.SortDirection.SORT_DIRECTION_ASCENDING)
            assertEquals("hey alice 1", messagesAsc[0].body)
            val messagesDesc =
                aliceConversation.messages(direction = MessageApiOuterClass.SortDirection.SORT_DIRECTION_DESCENDING)
            assertEquals("hey alice 3", messagesDesc[0].body)
        }
    }

    @Test
    fun testListBatchMessages() {
        val bobConversation =
            runBlocking { aliceClient.conversations.newConversation(bob.walletAddress) }
        val steveConversation = runBlocking {
            aliceClient.conversations.newConversation(fixtures.caro.walletAddress)
        }

        runBlocking { bobConversation.send(text = "hey alice 1") }
        runBlocking { bobConversation.send(text = "hey alice 2") }
        runBlocking { steveConversation.send(text = "hey alice 3") }
        val messages = runBlocking {
            aliceClient.conversations.listBatchMessages(
                listOf(
                    Pair(steveConversation.topic, null),
                    Pair(bobConversation.topic, null),
                ),
            )
        }
        val isSteveOrBobConversation = { topic: String ->
            (topic.equals(steveConversation.topic) || topic.equals(bobConversation.topic))
        }
        assertEquals(3, messages.size)
        assertTrue(isSteveOrBobConversation(messages[0].topic))
        assertTrue(isSteveOrBobConversation(messages[1].topic))
        assertTrue(isSteveOrBobConversation(messages[2].topic))
    }

    @Test
    fun testListBatchDecryptedMessages() {
        val bobConversation =
            runBlocking { aliceClient.conversations.newConversation(bob.walletAddress) }
        val steveConversation = runBlocking {
            aliceClient.conversations.newConversation(fixtures.caro.walletAddress)
        }

        runBlocking {
            bobConversation.send(text = "hey alice 1")
            bobConversation.send(text = "hey alice 2")
            steveConversation.send(text = "hey alice 3")
        }
        val messages = runBlocking {
            aliceClient.conversations.listBatchDecryptedMessages(
                listOf(
                    Pair(steveConversation.topic, null),
                    Pair(bobConversation.topic, null),
                ),
            )
        }
        assertEquals(3, messages.size)
    }

    @Test
    fun testListBatchMessagesWithPagination() {
        val bobConversation =
            runBlocking { aliceClient.conversations.newConversation(bob.walletAddress) }
        val steveConversation =
            runBlocking { aliceClient.conversations.newConversation(fixtures.caro.walletAddress) }

        runBlocking {
            bobConversation.send(text = "hey alice 1 bob")
            steveConversation.send(text = "hey alice 1 steve")
        }

        Thread.sleep(100)
        val date = Date()

        runBlocking {
            bobConversation.send(text = "hey alice 2 bob")
            bobConversation.send(text = "hey alice 3 bob")
            steveConversation.send(text = "hey alice 2 steve")
            steveConversation.send(text = "hey alice 3 steve")
        }

        val messages = runBlocking {
            aliceClient.conversations.listBatchMessages(
                listOf(
                    Pair(steveConversation.topic, Pagination(after = date)),
                    Pair(bobConversation.topic, Pagination(after = date)),
                ),
            )
        }

        assertEquals(4, messages.size)
    }

    @Test
    fun testImportV1ConversationFromJS() {
        val jsExportJSONData =
            (""" { "version": "v1", "peerAddress": "0x5DAc8E2B64b8523C11AF3e5A2E087c2EA9003f14", "createdAt": "2022-09-20T09:32:50.329Z" } """).toByteArray(
                StandardCharsets.UTF_8,
            )
        val conversation = aliceClient.importConversation(jsExportJSONData)
        assertEquals(conversation.peerAddress, "0x5DAc8E2B64b8523C11AF3e5A2E087c2EA9003f14")
    }

    @Test
    fun testImportV2ConversationFromJS() {
        val jsExportJSONData =
            (""" {"version":"v2","topic":"/xmtp/0/m-2SkdN5Qa0ZmiFI5t3RFbfwIS-OLv5jusqndeenTLvNg/proto","keyMaterial":"ATA1L0O2aTxHmskmlGKCudqfGqwA1H+bad3W/GpGOr8=","peerAddress":"0x436D906d1339fC4E951769b1699051f020373D04","createdAt":"2023-01-26T22:58:45.068Z","context":{"conversationId":"pat/messageid","metadata":{}}} """).toByteArray(
                StandardCharsets.UTF_8,
            )
        val conversation = aliceClient.importConversation(jsExportJSONData)
        assertEquals(conversation.peerAddress, "0x436D906d1339fC4E951769b1699051f020373D04")
    }

    @Test
    fun testImportV2ConversationWithNoContextFromJS() {
        val jsExportJSONData =
            (""" {"version":"v2","topic":"/xmtp/0/m-2SkdN5Qa0ZmiFI5t3RFbfwIS-OLv5jusqndeenTLvNg/proto","keyMaterial":"ATA1L0O2aTxHmskmlGKCudqfGqwA1H+bad3W/GpGOr8=","peerAddress":"0x436D906d1339fC4E951769b1699051f020373D04","createdAt":"2023-01-26T22:58:45.068Z"} """).toByteArray(
                StandardCharsets.UTF_8,
            )
        val conversation = aliceClient.importConversation(jsExportJSONData)
        assertEquals(conversation.peerAddress, "0x436D906d1339fC4E951769b1699051f020373D04")
    }

    @Test
    fun testCanStreamConversationsV2() = kotlinx.coroutines.test.runTest {
        bobClient.conversations.stream().test {
            val conversation = bobClient.conversations.newConversation(alice.walletAddress)
            conversation.send(content = "hi")
            assertEquals("hi", awaitItem().messages(limit = 1).first().body)
        }
    }

    @Test
    fun testStreamingMessagesFromV1Conversation() = kotlinx.coroutines.test.runTest {
        // Overwrite contact as legacy
        fixtures.publishLegacyContact(client = bobClient)
        fixtures.publishLegacyContact(client = aliceClient)
        val conversation = aliceClient.conversations.newConversation(bob.walletAddress)
        conversation.streamMessages().test {
            conversation.send("hi alice")
            assertEquals("hi alice", awaitItem().encodedContent.content.toStringUtf8())
        }
    }

    @Test
    fun testStreamingMessagesFromV2Conversations() = kotlinx.coroutines.test.runTest {
        val conversation = aliceClient.conversations.newConversation(bob.walletAddress)
        conversation.streamMessages().test {
            conversation.send("hi alice")
            assertEquals("hi alice", awaitItem().encodedContent.content.toStringUtf8())
        }
    }

    @Test
    fun testV2RejectsSpoofedContactBundles() {
        val topic = "/xmtp/0/m-Gdb7oj5nNdfZ3MJFLAcS4WTABgr6al1hePy6JV1-QUE/proto"
        val envelopeMessage =
            com.google.crypto.tink.subtle.Base64.decode("Er0ECkcIwNruhKLgkKUXEjsveG10cC8wL20tR2RiN29qNW5OZGZaM01KRkxBY1M0V1RBQmdyNmFsMWhlUHk2SlYxLVFVRS9wcm90bxLxAwruAwognstLoG6LWgiBRsWuBOt+tYNJz+CqCj9zq6hYymLoak8SDFsVSy+cVAII0/r3sxq7A/GCOrVtKH6J+4ggfUuI5lDkFPJ8G5DHlysCfRyFMcQDIG/2SFUqSILAlpTNbeTC9eSI2hUjcnlpH9+ncFcBu8StGfmilVGfiADru2fGdThiQ+VYturqLIJQXCHO2DkvbbUOg9xI66E4Hj41R9vE8yRGeZ/eRGRLRm06HftwSQgzAYf2AukbvjNx/k+xCMqti49Qtv9AjzxVnwttLiA/9O+GDcOsiB1RQzbZZzaDjQ/nLDTF6K4vKI4rS9QwzTJqnoCdp0SbMZFf+KVZpq3VWnMGkMxLW5Fr6gMvKny1e1LAtUJSIclI/1xPXu5nsKd4IyzGb2ZQFXFQ/BVL9Z4CeOZTsjZLGTOGS75xzzGHDtKohcl79+0lgIhAuSWSLDa2+o2OYT0fAjChp+qqxXcisAyrD5FB6c9spXKfoDZsqMV/bnCg3+udIuNtk7zBk7jdTDMkofEtE3hyIm8d3ycmxKYOakDPqeo+Nk1hQ0ogxI8Z7cEoS2ovi9+rGBMwREzltUkTVR3BKvgV2EOADxxTWo7y8WRwWxQ+O6mYPACsiFNqjX5Nvah5lRjihphQldJfyVOG8Rgf4UwkFxmI")
        val keyMaterial =
            com.google.crypto.tink.subtle.Base64.decode("R0BBM5OPftNEuavH/991IKyJ1UqsgdEG4SrdxlIG2ZY=")

        val conversation = ConversationV2(
            topic = topic,
            keyMaterial = keyMaterial,
            context = Context.newBuilder().build(),
            peerAddress = "0x2f25e33D7146602Ec08D43c1D6B1b65fc151A677",
            client = aliceClient,
            header = Invitation.SealedInvitationHeaderV1.newBuilder().build(),
        )
        val envelope = EnvelopeBuilder.buildFromString(
            topic = topic,
            timestamp = Date(),
            message = envelopeMessage,
        )
        assertThrows("pre-key not signed by identity key", XMTPException::class.java) {
            conversation.decodeEnvelope(envelope)
        }
    }

    @Test
    fun testCanPrepareV1Message() {
        // Publish legacy contacts so we can get v1 conversations
        fixtures.publishLegacyContact(client = bobClient)
        fixtures.publishLegacyContact(client = aliceClient)
        val conversation =
            runBlocking { aliceClient.conversations.newConversation(bob.walletAddress) }
        assertEquals(conversation.version, Conversation.Version.V1)
        val preparedMessage = conversation.prepareMessage(content = "hi")
        val messageID = preparedMessage.messageId
        runBlocking { conversation.send(prepared = preparedMessage) }
        val messages = runBlocking { conversation.messages() }
        val message = messages[0]
        assertEquals("hi", message.body)
        assertEquals(message.id, messageID)
    }

    @Test
    fun testCanPrepareV2Message() {
        val conversation =
            runBlocking { aliceClient.conversations.newConversation(bob.walletAddress) }
        val preparedMessage = conversation.prepareMessage(content = "hi")
        val messageID = preparedMessage.messageId
        runBlocking { conversation.send(prepared = preparedMessage) }
        val messages = runBlocking { conversation.messages() }
        val message = messages[0]
        assertEquals("hi", message.body)
        assertEquals(message.id, messageID)
    }

    @Test
    fun testCanSendPreparedMessageWithoutConversation() {
        val conversation =
            runBlocking { aliceClient.conversations.newConversation(bob.walletAddress) }
        val preparedMessage = conversation.prepareMessage(content = "hi")
        val messageID = preparedMessage.messageId

        // This does not need the `conversation` to `.publish` the message.
        // This simulates a background task publishing all pending messages upon connection.
        runBlocking { aliceClient.publish(envelopes = preparedMessage.envelopes) }

        val messages = runBlocking { conversation.messages() }
        val message = messages[0]
        assertEquals("hi", message.body)
        assertEquals(message.id, messageID)
    }

    @Test
    fun testFetchConversation() {
        // Generated from JS script
        val ints = arrayOf(
            31,
            116,
            198,
            193,
            189,
            122,
            19,
            254,
            191,
            189,
            211,
            215,
            255,
            131,
            171,
            239,
            243,
            33,
            4,
            62,
            143,
            86,
            18,
            195,
            251,
            61,
            128,
            90,
            34,
            126,
            219,
            236,
        )
        val bytes =
            ints.foldIndexed(ByteArray(ints.size)) { i, a, v -> a.apply { set(i, v.toByte()) } }

        val key = PrivateKey.newBuilder().also {
            it.secp256K1 = it.secp256K1.toBuilder().also { builder ->
                builder.bytes = bytes.toByteString()
            }.build()
            it.publicKey = it.publicKey.toBuilder().also { builder ->
                builder.secp256K1Uncompressed =
                    builder.secp256K1Uncompressed.toBuilder().also { keyBuilder ->
                        keyBuilder.bytes =
                            KeyUtil.addUncompressedByte(KeyUtil.getPublicKey(bytes)).toByteString()
                    }.build()
            }.build()
        }.build()

        val client = Client().create(account = PrivateKeyBuilder(key))
        assertEquals(client.apiClient.environment, XMTPEnvironment.DEV)
        runBlocking {
            val conversations = client.conversations.list()
            assertEquals(1, conversations.size)
            val topic = conversations[0].topic
            val conversation = client.fetchConversation(topic)
            assertEquals(conversations[0].topic, conversation?.topic)
            assertEquals(conversations[0].peerAddress, conversation?.peerAddress)

            val noConversation = client.fetchConversation("invalid_topic")
            assertEquals(null, noConversation)
        }
    }

    @Test
    fun testCanSendEncodedContentV1Message() {
        fixtures.publishLegacyContact(client = bobClient)
        fixtures.publishLegacyContact(client = aliceClient)
        val bobConversation =
            runBlocking { bobClient.conversations.newConversation(aliceWallet.address) }
        val aliceConversation =
            runBlocking { aliceClient.conversations.newConversation(bobWallet.address) }
        val encodedContent = TextCodec().encode(content = "hi")
        runBlocking { bobConversation.send(encodedContent = encodedContent) }
        val messages = runBlocking { aliceConversation.messages() }
        assertEquals(1, messages.size)
        assertEquals("hi", messages[0].content())
    }

    @Test
    fun testCanSendEncodedContentV2Message() {
        val bobConversation =
            runBlocking { bobClient.conversations.newConversation(aliceWallet.address) }
        val encodedContent = TextCodec().encode(content = "hi")
        runBlocking { bobConversation.send(encodedContent = encodedContent) }
        val messages = runBlocking { bobConversation.messages() }
        assertEquals(1, messages.size)
        assertEquals("hi", messages[0].content())
    }

    @Test
    fun testCanHaveConsentState() {
        val bobConversation =
            runBlocking { bobClient.conversations.newConversation(alice.walletAddress, null) }
        val isAllowed = bobConversation.consentState() == ConsentState.ALLOWED

        // Conversations you start should start as allowed
        assertTrue(isAllowed)
        assertTrue(bobClient.contacts.isAllowed(alice.walletAddress))

        runBlocking {
            bobClient.contacts.deny(listOf(alice.walletAddress))
            bobClient.contacts.refreshConsentList()
        }
        val isDenied = bobConversation.consentState() == ConsentState.DENIED
        assertEquals(bobClient.contacts.consentList.entries.size, 1)
        assertTrue(isDenied)

        val aliceConversation = runBlocking { aliceClient.conversations.list()[0] }
        val isUnknown = aliceConversation.consentState() == ConsentState.UNKNOWN

        // Conversations started with you should start as unknown
        assertTrue(isUnknown)

        runBlocking { aliceClient.contacts.allow(listOf(bob.walletAddress)) }

        val isBobAllowed = aliceConversation.consentState() == ConsentState.ALLOWED
        assertTrue(isBobAllowed)

        val aliceClient2 = Client().create(aliceWallet)
        val aliceConversation2 = runBlocking { aliceClient2.conversations.list()[0] }

        runBlocking { aliceClient2.contacts.refreshConsentList() }

        // Allow state should sync across clients
        val isBobAllowed2 = aliceConversation2.consentState() == ConsentState.ALLOWED

        assertTrue(isBobAllowed2)
    }

    @Test
    fun testCanHaveImplicitConsentOnMessageSend() {
        val bobConversation =
            runBlocking { bobClient.conversations.newConversation(alice.walletAddress, null) }
        val isAllowed = bobConversation.consentState() == ConsentState.ALLOWED

        // Conversations you start should start as allowed
        assertTrue(isAllowed)

        val aliceConversation = runBlocking { aliceClient.conversations.list()[0] }
        val isUnknown = aliceConversation.consentState() == ConsentState.UNKNOWN

        // Conversations you receive should start as unknown
        assertTrue(isUnknown)

        runBlocking {
            aliceConversation.send(content = "hey bob")
            aliceClient.contacts.refreshConsentList()
        }
        val isNowAllowed = aliceConversation.consentState() == ConsentState.ALLOWED

        // Conversations you send a message to get marked as allowed
        assertTrue(isNowAllowed)
    }

    @Test
    fun testCanPublishMultipleAddressConsentState() {
        runBlocking {
            val bobConversation = bobClient.conversations.newConversation(alice.walletAddress)
            val caroConversation =
                bobClient.conversations.newConversation(fixtures.caro.walletAddress)
            bobClient.contacts.refreshConsentList()
            Thread.sleep(1000)
            assertEquals(bobClient.contacts.consentList.entries.size, 2)
            assertTrue(bobConversation.consentState() == ConsentState.ALLOWED)
            assertTrue(caroConversation.consentState() == ConsentState.ALLOWED)
            bobClient.contacts.deny(listOf(alice.walletAddress, fixtures.caro.walletAddress))
            assertEquals(bobClient.contacts.consentList.entries.size, 2)
            assertTrue(bobConversation.consentState() == ConsentState.DENIED)
            assertTrue(caroConversation.consentState() == ConsentState.DENIED)
        }
    }

    @Test
    fun testCanValidateTopicsInsideConversation() {
        val validId = "sdfsadf095b97a9284dcd82b2274856ccac8a21de57bebe34e7f9eeb855fb21126d3b8f"

        // Creation of all known types of topics
        val privateStore = Topic.userPrivateStoreKeyBundle(validId).description
        val contact = Topic.contact(validId).description
        val userIntro = Topic.userIntro(validId).description
        val userInvite = Topic.userInvite(validId).description
        val directMessageV1 = Topic.directMessageV1(validId, "sd").description
        val directMessageV2 = Topic.directMessageV2(validId).description
        val preferenceList = Topic.preferenceList(validId).description

        // check if validation of topics accepts all types
        assertTrue(Topic.isValidTopic(privateStore))
        assertTrue(Topic.isValidTopic(contact))
        assertTrue(Topic.isValidTopic(userIntro))
        assertTrue(Topic.isValidTopic(userInvite))
        assertTrue(Topic.isValidTopic(directMessageV1))
        assertTrue(Topic.isValidTopic(directMessageV2))
        assertTrue(Topic.isValidTopic(preferenceList))
    }

    @Test
    fun testCannotValidateTopicsInsideConversation() {
        val invalidId = "��\\u0005�!\\u000b���5\\u00001\\u0007�蛨\\u001f\\u00172��.����K9K`�"

        // Creation of all known types of topics
        val privateStore = Topic.userPrivateStoreKeyBundle(invalidId).description
        val contact = Topic.contact(invalidId).description
        val userIntro = Topic.userIntro(invalidId).description
        val userInvite = Topic.userInvite(invalidId).description
        val directMessageV1 = Topic.directMessageV1(invalidId, "sd").description
        val directMessageV2 = Topic.directMessageV2(invalidId).description
        val preferenceList = Topic.preferenceList(invalidId).description

        // check if validation of topics no accept all types with invalid topic
        assertFalse(Topic.isValidTopic(privateStore))
        assertFalse(Topic.isValidTopic(contact))
        assertFalse(Topic.isValidTopic(userIntro))
        assertFalse(Topic.isValidTopic(userInvite))
        assertFalse(Topic.isValidTopic(directMessageV1))
        assertFalse(Topic.isValidTopic(directMessageV2))
        assertFalse(Topic.isValidTopic(preferenceList))
    }
}
