package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.protobuf.kotlin.toByteStringUtf8
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.ContentTypeId
import org.xmtp.android.library.codecs.ContentTypeIdBuilder
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.messages.walletAddress

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
        val aliceClient = fixtures.alixClient
        val aliceConversation = runBlocking {
            aliceClient.conversations.newConversation(fixtures.bo.walletAddress)
        }
        runBlocking {
            aliceConversation.send(
                content = 3.14,
                options = SendOptions(contentType = NumberCodec().contentType),
            )
        }
        val messages = runBlocking { aliceConversation.messages() }
        assertEquals(messages.size, 2)
        if (messages.size == 2) {
            val content: Double? = messages[0].content()
            assertEquals(3.14, content)
            assertEquals("Error: This app does not support numbers.", messages[0].fallbackContent)
        }
    }
}
