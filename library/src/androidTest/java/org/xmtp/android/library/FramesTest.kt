package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.frames.ConversationActionInputs
import org.xmtp.android.library.frames.DmActionInputs
import org.xmtp.android.library.frames.FrameActionInputs
import org.xmtp.android.library.frames.FramePostPayload
import org.xmtp.android.library.frames.FramesClient
import org.xmtp.android.library.frames.GetMetadataResponse
import java.net.HttpURLConnection
import java.net.URL

@RunWith(AndroidJUnit4::class)
class FramesTest {
    @Test
    fun testFramesClient() {
        val frameUrl = "https://fc-polls-five.vercel.app/polls/01032f47-e976-42ee-9e3d-3aac1324f4b8"
        val fixtures = fixtures()
        val aliceClient = fixtures.aliceClient

        val framesClient = FramesClient(xmtpClient = aliceClient)
        val conversationTopic = "foo"
        val participantAccountAddresses = listOf("alix", "bo")
        val metadata: GetMetadataResponse
        runBlocking {
            metadata = framesClient.proxy.readMetadata(url = frameUrl)
        }

        val dmInputs = DmActionInputs(
            conversationTopic = conversationTopic,
            participantAccountAddresses = participantAccountAddresses
        )
        val conversationInputs = ConversationActionInputs.Dm(dmInputs)
        val frameInputs = FrameActionInputs(
            frameUrl = frameUrl,
            buttonIndex = 1,
            inputText = null,
            state = null,
            conversationInputs = conversationInputs
        )
        val signedPayload: FramePostPayload
        runBlocking {
            signedPayload = framesClient.signFrameAction(inputs = frameInputs)
        }
        val postUrl = metadata.extractedTags["fc:frame:post_url"]
        assertNotNull(postUrl)
        val response: GetMetadataResponse
        runBlocking {
            response = framesClient.proxy.post(url = postUrl!!, payload = signedPayload)
        }

        assertEquals(response.extractedTags["fc:frame"], "vNext")

        val imageUrl = response.extractedTags["fc:frame:image"]
        assertNotNull(imageUrl)

        val mediaUrl = framesClient.proxy.mediaUrl(url = imageUrl!!)

        val url = URL(mediaUrl)
        val connection = url.openConnection() as HttpURLConnection
        connection.requestMethod = "GET"
        val responseCode = connection.responseCode
        assertEquals(responseCode, 200)
        assertEquals(connection.contentType, "image/png")
    }
}
