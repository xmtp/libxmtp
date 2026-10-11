package org.xmtp.android.example.messenger.feature.attachments

import org.xmtp.android.example.messenger.ActiveSession
import uniffi.xmtp_sdk.Conversation
import uniffi.xmtp_sdk.Message

/** Chat operations used by the retained attachment controller. */
internal interface AttachmentDestination {
    fun currentConversation(): Conversation?

    suspend fun reconcileFeatureMessage(
        owner: ActiveSession,
        token: Long,
        message: Message,
    )

    suspend fun refresh()

    fun openConversation(id: String)
}
