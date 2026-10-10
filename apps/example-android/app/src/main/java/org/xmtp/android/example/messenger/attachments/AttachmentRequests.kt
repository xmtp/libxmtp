package org.xmtp.android.example.messenger.attachments

import androidx.lifecycle.ViewModel
import org.xmtp.android.example.messenger.SessionKey

internal data class AttachmentRequest(
    val key: SessionKey,
    val token: Long,
    val value: String,
)

/** Retains only request identity across Activity recreation. */
internal class AttachmentRequests : ViewModel() {
    var picker: AttachmentRequest? = null
    var destination: AttachmentRequest? = null

    fun retain(key: SessionKey?) {
        if (picker?.key != key) picker = null
        if (destination?.key != key) destination = null
    }
}
