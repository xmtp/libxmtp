package org.xmtp.android.example.messenger.data.xmtp

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.messenger.feature.attachments.AttachmentController
import org.xmtp.android.example.messenger.feature.chat.ChatController
import org.xmtp.android.example.messenger.feature.conversations.ConversationsController
import org.xmtp.android.example.messenger.feature.conversations.CreateConversationController
import org.xmtp.android.example.messenger.feature.group.GroupController
import org.xmtp.android.example.messenger.feature.settings.SettingsController
import org.xmtp.android.example.messenger.notifications.NotificationController
import org.xmtp.android.example.messenger.session.SessionProjection
import uniffi.xmtp_sdk.*

/** Required projections finish before the reader can acknowledge its prior message. */
internal class FeatureMessageHandler(
    private val session: AppSession,
    private val conversations: ConversationsController,
    private val chat: ChatController,
    private val group: GroupController,
    private val settings: SettingsController,
    private val create: CreateConversationController,
    private val attachments: AttachmentController,
    private val notifications: NotificationController,
    private val scope: CoroutineScope,
) : SessionProjection {
    private val projection = Mutex()
    private var started = false
    internal var beforeActiveRefresh: suspend (ActiveSession) -> Unit = {}

    fun start() {
        if (started) return
        started = true
        scope.launch {
            combine(notifications.status, notifications.enabled) { status, enabled -> status to enabled }
                .collect { (status, enabled) -> settings.notificationStatus(status, enabled) }
        }
    }

    suspend fun activate(owner: ActiveSession?) =
        projection.withLock {
            chat.clear()
            conversations.clear()
            group.clear()
            create.clear()
            settings.clear()
            if (owner != null && session.accepts(owner.key)) {
                owner.work
                    .async {
                        beforeActiveRefresh(owner)
                        refresh(owner)
                    }.await()
            }
        }

    override suspend fun handle(
        owner: ActiveSession,
        message: Message,
    ) = projection.withLock {
        if (!session.accepts(owner.key)) return@withLock
        chat.onMessage(owner, message)
        if (!session.accepts(owner.key)) return@withLock
        conversations.refresh(owner)
        if (!session.accepts(owner.key)) return@withLock
        attachments.refresh(owner)
        if (!session.accepts(owner.key)) throw CancellationException("Session changed during message handling")
    }

    override suspend fun invalidate(owner: ActiveSession) = projection.withLock { refresh(owner) }

    private suspend fun refresh(owner: ActiveSession) {
        if (!session.accepts(owner.key)) return
        conversations.refresh(owner)
        if (!session.accepts(owner.key)) return
        chat.refresh(owner)
        if (!session.accepts(owner.key)) return
        group.refresh(owner)
        if (!session.accepts(owner.key)) return
        settings.refresh(owner)
        if (!session.accepts(owner.key)) return
        attachments.refresh(owner)
    }
}
