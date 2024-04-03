package org.xmtp.android.library.frames

import org.xmtp.android.library.frames.FramesConstants.OPEN_FRAMES_PROXY_URL
import java.net.URI

class OpenFramesProxy(private val inner: ProxyClient = ProxyClient(OPEN_FRAMES_PROXY_URL)) {

    suspend fun readMetadata(url: String): GetMetadataResponse {
        return inner.readMetadata(url)
    }

    suspend fun post(url: String, payload: FramePostPayload): GetMetadataResponse {
        return inner.post(url, payload)
    }

    suspend fun postRedirect(url: String, payload: FramePostPayload): FramesApiRedirectResponse {
        return inner.postRedirect(url, payload)
    }

    fun mediaUrl(url: String): String {
        if (URI(url).scheme == "data") {
            return url
        } else {
            return inner.mediaUrl(url)
        }
    }
}
