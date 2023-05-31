package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.protobuf.kotlin.toByteStringUtf8
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.CompositeCodec
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.ContentTypeId
import org.xmtp.android.library.codecs.ContentTypeIdBuilder
import org.xmtp.android.library.codecs.DecodedComposite
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.messages.walletAddress

data class NumberCodec(
    override var contentType: ContentTypeId = ContentTypeIdBuilder.builderFromAuthorityId(
        authorityId = "example.com",
        typeId = "number",
        versionMajor = 1,
        versionMinor = 1
    )
) : ContentCodec<Double> {
    override fun encode(content: Double): EncodedContent {
        return EncodedContent.newBuilder().also {
            it.type = ContentTypeIdBuilder.builderFromAuthorityId(
                authorityId = "example.com",
                typeId = "number",
                versionMajor = 1,
                versionMinor = 1
            )
            it.content = mapOf(Pair("number", content)).toString().toByteStringUtf8()
        }.build()
    }

    override fun decode(content: EncodedContent): Double =
        content.content.toStringUtf8().filter { it.isDigit() || it == '.' }.toDouble()
}
@RunWith(AndroidJUnit4::class)
class CodecTest {

    @Test
    fun testCanRoundTripWithCustomContentType() {
        Client.register(codec = NumberCodec())
        val fixtures = fixtures()
        val aliceClient = fixtures.aliceClient
        val aliceConversation =
            aliceClient.conversations.newConversation(fixtures.bob.walletAddress)
        aliceConversation.send(
            content = 3.14,
            options = SendOptions(contentType = NumberCodec().contentType)
        )
        val messages = aliceConversation.messages()
        assertEquals(messages.size, 1)
        if (messages.size == 1) {
            val content: Double? = messages[0].content()
            assertEquals(3.14, content)
        }
    }

    @Test
    fun testCompositeCodecOnePart() {
        Client.register(codec = CompositeCodec())
        val fixtures = fixtures()
        val aliceClient = fixtures.aliceClient
        val aliceConversation =
            aliceClient.conversations.newConversation(fixtures.bob.walletAddress)
        val textContent = TextCodec().encode(content = "hiya")
        val source = DecodedComposite(encodedContent = textContent)
        aliceConversation.send(
            content = source,
            options = SendOptions(contentType = CompositeCodec().contentType)
        )
        val messages = aliceConversation.messages()
        val decoded: DecodedComposite? = messages[0].content()
        assertEquals("hiya", decoded?.content())
    }

    @Test
    fun testCompositeCodecCanHaveParts() {
        Client.register(codec = CompositeCodec())
        Client.register(codec = NumberCodec())
        val fixtures = fixtures()
        val aliceClient = fixtures.aliceClient!!
        val aliceConversation =
            aliceClient.conversations.newConversation(fixtures.bob.walletAddress)
        val textContent = TextCodec().encode(content = "sup")
        val numberContent = NumberCodec().encode(content = 3.14)
        val source = DecodedComposite(
            parts = listOf(
                DecodedComposite(encodedContent = textContent),
                DecodedComposite(parts = listOf(DecodedComposite(encodedContent = numberContent)))
            )
        )
        aliceConversation.send(
            content = source,
            options = SendOptions(contentType = CompositeCodec().contentType)
        )
        val messages = aliceConversation.messages()
        val decoded: DecodedComposite? = messages[0].content()
        val part1 = decoded!!.parts[0]
        val part2 = decoded.parts[1].parts[0]
        assertEquals("sup", part1.content())
        assertEquals(3.14, part2.content())
    }
}
