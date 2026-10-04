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
            val legacy =
                EncodedContent(
                    type,
                    mapOf("action" to "added", "reference" to reference, "schema" to "shortcode"),
                    content = "smile".toByteArray(),
                )
            val expected =
                MessageContent.Reaction(
                    reference,
                    null,
                    Reaction("smile", ReactionAction.ADDED, ReactionSchema.SHORTCODE),
                )
            assertEquals(expected, client.decodeContent(canonical))
            assertEquals(expected, client.decodeContent(legacy))
        }

    private suspend fun send(v2: Boolean): Pair<Message, Message> {
        val fixtures = createFixtures()
        val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
        val parent = dm.sendText("hey alice 2 bob")
        val id =
            if (v2) {
                dm.sendReaction(parent, fixtures.alixClient.inboxId(), reaction)
            } else {
                dm.send(
                    EncodedContent(
                        ContentTypeId("xmtp.org", "reaction", 1u, 0u),
                        mapOf("action" to "added", "reference" to parent, "schema" to "unicode"),
                        content = reaction.content.toByteArray(),
                    ),
                )
            }
        val messages = dm.messages()
        return messages.single { it.id == id } to messages.single { it.id == parent }
    }

    @Test fun testCanUseReactionCodec() =
        runBlocking {
            val (message, parent) = send(false)
            val content = (message.content as SDKMessageContent.Standard).value as MessageContent.Reaction
            assertEquals(parent.id, content.reference)
            assertEquals(reaction, content.reaction)
        }

    @Test fun testCanUseReactionV2Codec() =
        runBlocking {
            val (message, parent) = send(true)
            val content = (message.content as SDKMessageContent.Standard).value as MessageContent.Reaction
            assertEquals(parent.id, content.reference)
            assertEquals(reaction, content.reaction)
            assertEquals(listOf(reaction), parent.reactions.map { it.reaction })
        }

    @Test fun testCanMixReactionTypes() =
        runBlocking {
            val fixtures = createFixtures()
            val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
            val parent = dm.sendText("parent")
            dm.sendReaction(parent, fixtures.alixClient.inboxId(), reaction)
            dm.send(
                EncodedContent(
                    ContentTypeId("xmtp.org", "reaction", 1u, 0u),
                    mapOf("action" to "added", "reference" to parent, "schema" to "unicode"),
                    content = "U+1F604".toByteArray(),
                ),
            )
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
