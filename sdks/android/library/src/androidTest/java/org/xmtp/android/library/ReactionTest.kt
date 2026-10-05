package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class ReactionTest : BaseInstrumentedTest() {
    private val reaction = Reaction("U+1F603", ReactionAction.ADDED, ReactionSchema.UNICODE)

    @Test fun testCanDecodeLegacyForm() =
        runBlocking {
            val client = createClient(createWallet())
            val reference = "ab".repeat(32)
            val type = ContentTypeId("xmtp.org", "reaction", 1u, 0u)
            val canonical =
                EncodedContent(
                    type,
                    content =
                        (
                            "{\"action\":\"added\",\"content\":\"smile\"," +
                                "\"reference\":\"$reference\",\"schema\":\"shortcode\"}"
                        ).toByteArray(),
                )
            val expected =
                MessageContent.Reaction(
                    reference,
                    null,
                    Reaction("smile", ReactionAction.ADDED, ReactionSchema.SHORTCODE),
                )
            assertEquals(expected, client.decodeContent(canonical))
            val codec = ReactionV2Codec()
            val current = ReactionV2Content(reference, null, expected.reaction)
            assertEquals(ContentTypeId("xmtp.org", "reaction", 2u, 0u), codec.type)
            assertEquals(current, codec.decode(codec.encode(current)))
        }

    private suspend fun send(v2: Boolean): Pair<ReactionMessage, Message> {
        val fixtures = createFixtures()
        val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
        val parent = dm.sendText("hey alice 2 bob")
        val id =
            if (v2) {
                dm.sendReaction(parent, fixtures.alixClient.inboxId(), reaction)
            } else {
                val codec = ReactionV2Codec()
                val payload = ReactionV2Content(parent, fixtures.alixClient.inboxId(), reaction)
                assertEquals(payload, codec.decode(codec.encode(payload)))
                dm.send(codec, payload)
            }
        dm.sync()
        val messages = dm.messages()
        assertFalse(messages.any { it.id == id })
        val storedParent = messages.single { it.id == parent }
        return storedParent.reactions.single { it.id == id } to storedParent
    }

    @Test fun testCanUseReactionCodec() =
        runBlocking {
            val (message, parent) = send(false)
            assertEquals(parent.senderInboxId, message.senderInboxId)
            assertEquals(reaction, message.reaction)
        }

    @Test fun testCanUseReactionV2Codec() =
        runBlocking {
            val (message, parent) = send(true)
            assertEquals(parent.senderInboxId, message.senderInboxId)
            assertEquals(reaction, message.reaction)
            assertEquals(listOf(reaction), parent.reactions.map { it.reaction })
        }

    @Test fun testCanMixReactionTypes() =
        runBlocking {
            val fixtures = createFixtures()
            val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
            val parent = dm.sendText("parent")
            val firstId = dm.sendReaction(parent, fixtures.alixClient.inboxId(), reaction)
            val secondId =
                dm.send(
                    ReactionV2Codec(),
                    ReactionV2Content(
                        parent,
                        fixtures.alixClient.inboxId(),
                        Reaction("U+1F604", ReactionAction.ADDED, ReactionSchema.UNICODE),
                    ),
                )
            dm.sync()
            val stored = dm.messages().single { it.id == parent }.reactions
            assertEquals(2, stored.size)
            assertEquals(setOf(firstId, secondId), stored.map { it.id }.toSet())
            assertEquals(
                setOf("U+1F603", "U+1F604"),
                dm
                    .messages()
                    .single {
                        it.id == parent
                    }.reactions
                    .map { it.reaction.content }
                    .toSet(),
            )
        }
}
