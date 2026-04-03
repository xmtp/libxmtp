package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.protobuf.kotlin.toByteStringUtf8
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentTypeReaction
import org.xmtp.android.library.codecs.ContentTypeReactionV2
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.Reaction
import org.xmtp.android.library.codecs.ReactionAction
import org.xmtp.android.library.codecs.ReactionCodec
import org.xmtp.android.library.codecs.ReactionSchema
import org.xmtp.android.library.codecs.ReactionV2Codec
import org.xmtp.android.library.libxmtp.DecodedMessage

@RunWith(AndroidJUnit4::class)
class ReactionTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var alixClient: Client
    private lateinit var boClient: Client

    @org.junit.Before
    override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
    }

    @Test
    fun testCanDecodeLegacyForm() {
        val codec = ReactionCodec()

        // This is how clients send reactions now.
        val canonicalEncoded =
            EncodedContent
                .newBuilder()
                .also {
                    it.type = ContentTypeReaction
                    it.content =
                        """
                        {
                            "action": "added",
                            "content": "smile",
                            "reference": "abc123",
                            "schema": "shortcode"
                        }
                        """.trimIndent()
                            .toByteStringUtf8()
                }.build()

        // Previously, some clients sent reactions like this.
        // So we test here to make sure we can still decode them.
        val legacyEncoded =
            EncodedContent
                .newBuilder()
                .also {
                    it.type = ContentTypeReaction
                    it.putAllParameters(
                        mapOf(
                            "action" to "added",
                            "reference" to "abc123",
                            "schema" to "shortcode",
                        ),
                    )
                    it.content = "smile".toByteStringUtf8()
                }.build()

        val canonical = codec.decode(canonicalEncoded)
        val legacy = codec.decode(legacyEncoded)

        assertEquals(ReactionAction.Added, canonical.action)
        assertEquals(ReactionAction.Added, legacy.action)
        assertEquals("smile", canonical.content)
        assertEquals("smile", legacy.content)
        assertEquals("abc123", canonical.reference)
        assertEquals("abc123", legacy.reference)
        assertEquals(ReactionSchema.Shortcode, canonical.schema)
        assertEquals(ReactionSchema.Shortcode, legacy.schema)
    }

    @Test
    fun testCanUseReactionCodec() {
        Client.register(codec = ReactionCodec())

        val alixConversation =
            runBlocking {
                alixClient.conversations.newConversation(boClient.inboxId)
            }

        runBlocking { alixConversation.send(text = "hey alice 2 bob") }

        val messageToReact = runBlocking { alixConversation.messages()[0] }

        val attachment =
            Reaction(
                reference = messageToReact.id,
                action = ReactionAction.Added,
                content = "U+1F603",
                schema = ReactionSchema.Unicode,
            )

        runBlocking {
            alixConversation.send(
                content = attachment,
                options = SendOptions(contentType = ContentTypeReaction),
            )
        }
        val messages = runBlocking { alixConversation.messages() }
        assertEquals(messages.size, 3)
        if (messages.size == 3) {
            val content: Reaction? = messages.first().content()
            assertEquals("U+1F603", content?.content)
            assertEquals(messageToReact.id, content?.reference)
            assertEquals(ReactionAction.Added, content?.action)
            assertEquals(ReactionSchema.Unicode, content?.schema)
        }
    }

    @Test
    fun testCanUseReactionV2Codec() {
        Client.register(codec = ReactionV2Codec())

        val alixConversation =
            runBlocking {
                alixClient.conversations.newConversation(boClient.inboxId)
            }

        runBlocking { alixConversation.send(text = "hey alice 2 bob") }

        val messageToReact = runBlocking { alixConversation.messages()[0] }

        val reaction =
            Reaction(
                reference = messageToReact.id,
                action = ReactionAction.Added,
                content = "U+1F603",
                schema = ReactionSchema.Unicode,
                referenceInboxId = alixClient.inboxId,
            )

        runBlocking {
            alixConversation.send(
                content = reaction,
                options = SendOptions(contentType = ContentTypeReactionV2),
            )
        }
        val messages = runBlocking { alixConversation.messages() }
        assertEquals(messages.size, 3)
        if (messages.size == 3) {
            val content: Reaction? = messages.first().content()
            assertEquals("U+1F603", content?.content)
            assertEquals(messageToReact.id, content?.reference)
            assertEquals(ReactionAction.Added, content?.action)
            assertEquals(ReactionSchema.Unicode, content?.schema)
        }

        val messagesWithReactions: List<DecodedMessage> =
            runBlocking {
                alixConversation.messagesWithReactions()
            }
        assertEquals(messagesWithReactions.size, 2)
        assertEquals(messagesWithReactions[0].id, messageToReact.id)
        val reactionContent: Reaction? =
            messagesWithReactions[0].childMessages!![0].let { it.content()!! }
        assertEquals(reactionContent?.reference, messageToReact.id)
    }

    @Test
    fun testCanMixReactionTypes() =
        runBlocking {
            // Register both codecs
            Client.register(codec = ReactionV2Codec())
            Client.register(codec = ReactionCodec())

            val alixConversation = alixClient.conversations.newConversation(boClient.inboxId)

            // Send initial message
            alixConversation.send(text = "hey alice 2 bob")
            val messageToReact = alixConversation.messages()[0]

            // Send V2 reaction
            val reactionV2 =
                Reaction(
                    reference = messageToReact.id,
                    action = ReactionAction.Added,
                    content = "U+1F603",
                    schema = ReactionSchema.Unicode,
                    referenceInboxId = alixClient.inboxId,
                )
            alixConversation.send(
                content = reactionV2,
                options = SendOptions(contentType = ContentTypeReactionV2),
            )

            // Send V1 reaction
            val reactionV1 =
                Reaction(
                    reference = messageToReact.id,
                    action = ReactionAction.Added,
                    content = "U+1F604", // Different emoji to distinguish
                    schema = ReactionSchema.Unicode,
                )
            alixConversation.send(
                content = reactionV1,
                options = SendOptions(contentType = ContentTypeReaction),
            )

            // Verify both reactions appear in messagesWithReactions
            val messagesWithReactions = runBlocking { alixConversation.messagesWithReactions() }

            assertEquals(2, messagesWithReactions.size)
            assertEquals(messageToReact.id, messagesWithReactions[0].id)
            assertEquals(2, messagesWithReactions[0].childMessages!!.size)

            // Verify both reaction contents
            val childContents =
                messagesWithReactions[0]
                    .childMessages!!
                    .mapNotNull {
                        val content = it.content<Reaction>()
                        content?.content
                    }.toSet()
            assertEquals(setOf("U+1F603", "U+1F604"), childContents)
        }
}
