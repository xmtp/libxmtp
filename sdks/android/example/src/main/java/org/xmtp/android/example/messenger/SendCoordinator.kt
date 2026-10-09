package org.xmtp.android.example.messenger
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import uniffi.xmtp_sdk.*
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

/** Persist acceptance before publication
. A retry never calls the typed queue action. */
class SendCoordinator(
    private val preferences: MessengerPreferences,
    private val accepts: (SessionKey) -> Boolean,
    private val admit: (SessionKey, () -> Unit) -> Boolean = { key, change ->
        if (accepts(key)) {
            change()
            true
        } else {
            false
        }
    },
) {
    internal var beforePublish: suspend (MessageId) -> Unit = {}
    internal var messageRead: suspend (SDKClient, MessageId) -> Message? = { client, id ->
        client.conversations.getMessageById(id)
    }

    private val activeDrafts =
        ConcurrentHashMap.newKeySet<String>()

    fun isInFlight(id: String) =
        activeDrafts
            .contains(id)

    suspend fun queue(
        key: SessionKey,
        client: SDKClient,
        conversation: Conversation,
        draft: SendDraftRef =
            SendDraftRef(
                UUID
                    .randomUUID()
                    .toString(),
                conversation
                    .id(),
            ),
        admission: () -> Boolean = { true },
        onAccepted: (MessageId) -> Unit = {},
        reconcile: suspend (Message) -> Unit,
        send: suspend () -> MessageId,
    ): MessageId {
        check(accepts(key))
        if (!admission()) throw kotlinx.coroutines.CancellationException("Action scope changed")
        check(
            activeDrafts
                .add(
                    draft.draftId,
                ),
        ) {
            "This draft is already being sent"
        }
        try {
            val prepared =
                preferences
                    .saveDraft(
                        key.profileId,
                        draft
                            .copy(
                                phase =
                                    SendPhase.QUEUEING,
                            ),
                        admit = { change -> admit(key, change) },
                    )
            if (!prepared) throw kotlinx.coroutines.CancellationException("Session changed before queue admission")
            if (!accepts(key) || !admission()) {
                preferences.removeDraft(key.profileId, draft.draftId, admit = { change -> admit(key, change) })
                throw kotlinx.coroutines.CancellationException("Action scope changed before send")
            }
            val id = send()
            // Acceptance belongs to this profile even when navigation cancels the caller.
            withContext(NonCancellable) {
                onAccepted(id)
                if (accepts(key)) {
                    preferences
                        .saveDraft(
                            key.profileId,
                            draft
                                .copy(
                                    phase =
                                        SendPhase.ACCEPTED,
                                    acceptedMessageId = id,
                                ),
                            admit = { change -> admit(key, change) },
                        )
                }
            }
            if (!accepts(key)) return id
            client.conversations
                .getMessageById(id)
                ?.let {
                    reconcile(it)
                }
            retry(
                key,
                client,
                conversation,
                id,
                admission = admission,
                reconcile = reconcile,
            )
            return id
        } finally {
            activeDrafts
                .remove(
                    draft.draftId,
                )
        }
    }

    suspend fun retry(
        key: SessionKey,
        client: SDKClient,
        conversation: Conversation,
        id: MessageId,
        admission: () -> Boolean = { true },
        reconcile: suspend (Message) -> Unit,
    ) {
        check(accepts(key))
        if (!admission()) throw kotlinx.coroutines.CancellationException("Action scope changed")
        val current = messageRead(client, id) ?: error("Stored message is unavailable")
        if (!accepts(key) ||
            !admission()
        ) {
            throw kotlinx.coroutines.CancellationException("Action scope changed before publication")
        }
        require(current.conversationId == conversation.id()) { "Message belongs to another conversation" }
        if (current.deliveryStatus !=
            DeliveryStatus.PUBLISHED
        ) {
            beforePublish(id)
            if (!accepts(key) ||
                !admission()
            ) {
                throw kotlinx.coroutines.CancellationException("Action scope changed before publication")
            }
            conversation
                .publishMessage(id)
        }
        if (!accepts(key)) return
        client.conversations
            .getMessageById(id)
            ?.let {
                reconcile(it)
            }
        preferences
            .drafts(
                key.profileId,
            ).filter {
                it.acceptedMessageId == id
            }.forEach {
                preferences
                    .removeDraft(
                        key.profileId,
                        it.draftId,
                        admit = { change -> admit(key, change) },
                    )
            }
    }
}
