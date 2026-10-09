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
    internal var afterQueueDraftCommit: suspend () -> Unit = {}

    private val activeDrafts =
        ConcurrentHashMap.newKeySet<String>()

    private data class KnownAcceptance(
        val profile: String,
        val commit: AcceptedMessageCommit,
    )

    private val knownAcceptances = ConcurrentHashMap<String, KnownAcceptance>()

    fun knowsAccepted(id: String) = knownAcceptances.containsKey(id)

    suspend fun recoverAccepted(key: SessionKey) {
        for ((draftId, entry) in knownAcceptances.entries.toList()) {
            if (entry.profile != key.profileId || isInFlight(draftId)) continue
            if (!accepts(key)) return
            val drafts = preferences.drafts(key.profileId)
            if (!accepts(key)) return
            if (drafts.none { it.draftId == draftId }) {
                knownAcceptances.remove(draftId, entry)
                continue
            }
            if (entry.commit.finish()) knownAcceptances.remove(draftId, entry)
        }
    }

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
        onStored: (MessageId) -> Unit = {},
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
            val (prepared, previous) =
                preferences.prepareQueueDraft(key.profileId, draft) { change ->
                    var scoped = false
                    val accepted =
                        admit(key) {
                            if (admission()) {
                                change()
                                scoped = true
                            }
                        }
                    accepted && scoped
                }
            if (!prepared) throw kotlinx.coroutines.CancellationException("Session changed before queue admission")
            afterQueueDraftCommit()
            if (!accepts(key) || !admission()) {
                preferences.rejectQueueDraft(
                    key.profileId,
                    draft.copy(phase = SendPhase.QUEUEING),
                    previous,
                    admit = { change -> admit(key, change) },
                )
                throw kotlinx.coroutines.CancellationException("Action scope changed before send")
            }
            val id = send()
            // Acceptance belongs to this profile even when navigation cancels the caller.
            withContext(NonCancellable) {
                val acceptedDraft = draft.copy(phase = SendPhase.ACCEPTED, acceptedMessageId = id)
                val entry =
                    KnownAcceptance(
                        key.profileId,
                        AcceptedMessageCommit(
                            id,
                            persist = { preferences.saveAcceptedDraft(key.profileId, acceptedDraft) },
                            verify = {
                                preferences.drafts(key.profileId).any {
                                    it.draftId == draft.draftId && it.phase == SendPhase.ACCEPTED &&
                                        it.acceptedMessageId == id
                                }
                            },
                            acknowledge = { onAccepted(id) },
                        ),
                    )
                knownAcceptances[draft.draftId] = entry
                onStored(id)
                if (entry.commit.finish()) knownAcceptances.remove(draft.draftId, entry)
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
