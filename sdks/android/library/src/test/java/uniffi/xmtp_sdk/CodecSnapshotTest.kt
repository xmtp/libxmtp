package uniffi.xmtp_sdk

import org.junit.Assert.assertArrayEquals
import org.junit.Test

class CodecSnapshotTest {
    @Test
    fun encodedSendKeepsBytesAfterCodecMutation() {
        val bytes = byteArrayOf(1, 2, 3)
        val codec =
            object : ContentCodec<String> {
                override val type = ContentTypeId("example.com", "snapshot", 1u, 0u)

                override fun encode(value: String) = EncodedContent(type, content = bytes)

                override fun decode(encoded: EncodedContent) = "unused"
            }
        val encoded = encodeForSend(codec, "value")
        bytes[0] = 99
        assertArrayEquals(byteArrayOf(1, 2, 3), encoded.content)
    }
}
