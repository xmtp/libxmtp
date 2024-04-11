package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.protobuf.kotlin.toByteStringUtf8
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.Crypto.Companion.verifyHmacSignature
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.ContentTypeId
import org.xmtp.android.library.codecs.ContentTypeIdBuilder
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.messages.InvitationV1ContextBuilder
import org.xmtp.android.library.messages.MessageV2Builder
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress
import java.time.Instant

data class NumberCodec(
    override var contentType: ContentTypeId = ContentTypeIdBuilder.builderFromAuthorityId(
        authorityId = "example.com",
        typeId = "number",
        versionMajor = 1,
        versionMinor = 1,
    ),
) : ContentCodec<Double> {
    override fun encode(content: Double): EncodedContent {
        return EncodedContent.newBuilder().also {
            it.type = ContentTypeIdBuilder.builderFromAuthorityId(
                authorityId = "example.com",
                typeId = "number",
                versionMajor = 1,
                versionMinor = 1,
            )
            it.content = mapOf(Pair("number", content)).toString().toByteStringUtf8()
        }.build()
    }

    override fun decode(content: EncodedContent): Double =
        content.content.toStringUtf8().filter { it.isDigit() || it == '.' }.toDouble()

    override fun shouldPush(content: Double): Boolean = false

    override fun fallback(content: Double): String? {
        return "Error: This app does not support numbers."
    }
}

@RunWith(AndroidJUnit4::class)
class CodecTest {

    @Test
    fun testCanRoundTripWithCustomContentType() {
        Client.register(codec = NumberCodec())
        val fixtures = fixtures()
        val aliceClient = fixtures.aliceClient
        val aliceConversation = runBlocking {
            aliceClient.conversations.newConversation(fixtures.bob.walletAddress)
        }
        runBlocking {
            aliceConversation.send(
                content = 3.14,
                options = SendOptions(contentType = NumberCodec().contentType),
            )
        }
        val messages = runBlocking { aliceConversation.messages() }
        assertEquals(messages.size, 1)
        if (messages.size == 1) {
            val content: Double? = messages[0].content()
            assertEquals(3.14, content)
            assertEquals("Error: This app does not support numbers.", messages[0].fallbackContent)
        }
    }

    @Test
    fun testCanGetPushInfoBeforeDecoded() {
        val codec = NumberCodec()
        Client.register(codec = codec)
        val fixtures = fixtures()
        val aliceClient = fixtures.aliceClient
        val aliceConversation = runBlocking {
            aliceClient.conversations.newConversation(fixtures.bob.walletAddress)
        }
        runBlocking {
            aliceConversation.send(
                content = 3.14,
                options = SendOptions(contentType = codec.contentType),
            )
        }
        val messages = runBlocking { aliceConversation.messages() }
        assert(messages.isNotEmpty())

        val message = MessageV2Builder.buildEncode(
            client = aliceClient,
            encodedContent = messages[0].encodedContent,
            topic = aliceConversation.topic,
            keyMaterial = aliceConversation.keyMaterial!!,
            codec = codec,
        )

        assertEquals(false, message.shouldPush)
        assertEquals(true, message.senderHmac?.isNotEmpty())
    }

    @Test
    fun testReturnsAllHMACKeys() {
        val alix = PrivateKeyBuilder()
        val clientOptions =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val alixClient = Client().create(alix, clientOptions)
        val conversations = mutableListOf<Conversation>()
        repeat(5) {
            val account = PrivateKeyBuilder()
            val client = Client().create(account, clientOptions)
            runBlocking {
                conversations.add(
                    alixClient.conversations.newConversation(
                        client.address,
                        context = InvitationV1ContextBuilder.buildFromConversation(conversationId = "hi")
                    )
                )
            }
        }

        val thirtyDayPeriodsSinceEpoch = Instant.now().epochSecond / 60 / 60 / 24 / 30

        val hmacKeys = alixClient.conversations.getHmacKeys()

        val topics = hmacKeys.hmacKeysMap.keys
        conversations.forEach { convo ->
            assertTrue(topics.contains(convo.topic))
        }

        val topicHmacs = mutableMapOf<String, ByteArray>()
        val headerBytes = ByteArray(10)

        conversations.forEach { conversation ->
            val topic = conversation.topic
            val payload = TextCodec().encode(content = "Hello, world!")

            val message = MessageV2Builder.buildEncode(
                client = alixClient,
                encodedContent = payload,
                topic = topic,
                keyMaterial = headerBytes,
                codec = TextCodec()
            )

            val keyMaterial = conversation.keyMaterial
            val info = "$thirtyDayPeriodsSinceEpoch-${alixClient.address}"
            val key = Crypto.deriveKey(keyMaterial!!, ByteArray(0), info.toByteArray())
            val hmac = Crypto.calculateMac(key, headerBytes)

            topicHmacs[topic] = hmac
        }

        hmacKeys.hmacKeysMap.forEach { (topic, hmacData) ->
            hmacData.valuesList.forEachIndexed { idx, hmacKeyThirtyDayPeriod ->
                val valid = verifyHmacSignature(
                    hmacKeyThirtyDayPeriod.hmacKey.toByteArray(),
                    topicHmacs[topic]!!,
                    headerBytes
                )
                assertTrue(valid == (idx == 1))
            }
        }
    }
}
