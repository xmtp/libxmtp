package org.xmtp.android.example.messenger

import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import uniffi.xmtp_sdk.*

/** Persist acceptance before publication. A retry never calls the typed queue action. */
class SendCoordinator(private val preferences: MessengerPreferences, private val accepts: (SessionKey) -> Boolean) {
    suspend fun queue(key: SessionKey, client: SDKClient, conversation: Conversation, draft: SendDraftRef = SendDraftRef(UUID.randomUUID().toString(), conversation.id()), reconcile: suspend (Message) -> Unit, send: suspend () -> MessageId): MessageId {
        check(accepts(key))
        preferences.saveDraft(key.profileId, draft.copy(phase = SendPhase.QUEUEING))
        val id = send()
        // Acceptance belongs to this profile even when navigation cancels the caller.
        withContext(NonCancellable) {
            if (accepts(key)) preferences.saveDraft(key.profileId, draft.copy(phase = SendPhase.ACCEPTED, acceptedMessageId = id))
        }
        if (!accepts(key)) return id
        client.conversations.getMessageById(id)?.let { reconcile(it) }
        retry(key, client, conversation, id, reconcile)
        return id
    }
    suspend fun retry(key: SessionKey, client: SDKClient, conversation: Conversation, id: MessageId, reconcile: suspend (Message) -> Unit) {
        check(accepts(key))
        val current = client.conversations.getMessageById(id) ?: error("Stored message is unavailable")
        if (current.deliveryStatus != DeliveryStatus.PUBLISHED) conversation.publishMessage(id)
        if (!accepts(key)) return
        client.conversations.getMessageById(id)?.let { reconcile(it) }
        preferences.drafts(key.profileId).filter { it.acceptedMessageId == id }.forEach { preferences.removeDraft(key.profileId, it.draftId) }
    }
}
