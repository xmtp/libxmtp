package org.xmtp.android.example.messenger.feature.chat

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.sync.withPermit
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.messenger.app.DestinationOrigin
import org.xmtp.android.example.messenger.app.DestinationScope
import org.xmtp.android.example.messenger.data.xmtp.ChatQueries
import org.xmtp.android.example.messenger.data.xmtp.ConversationQueries
import org.xmtp.android.example.messenger.feature.*
import org.xmtp.android.example.messenger.feature.attachments.AttachmentDestination
import org.xmtp.android.example.messenger.feature.conversations.ConversationsController
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.chat.*
import uniffi.xmtp_sdk.*
import java.util.UUID
import java.util.concurrent.atomic.AtomicLong

internal class ChatController(
    private val session: AppSession,
    private val queries: ChatQueries,
    private val conversations: ConversationQueries,
    private val lists: ConversationsController,
    private val drafts: SecureComposerDrafts,
    private val sends: SendCoordinator,
    private val reads: Semaphore,
    private val destinations: DestinationScope,
    private val scope: CoroutineScope,
) : AttachmentDestination {
    private val ui = MutableStateFlow(ChatState())
    val state: StateFlow<ChatState> = ui
    private val projection = Mutex()
    private val cache = SDKTranscriptCache<Message>({ it.id }, { it.historyPosition() })
    private val draftLock = Any()
    private val composerDrafts = mutableMapOf<Pair<String, String>, ComposerDraft>()
    private val requests = AtomicLong()
    private val openAttempts = AtomicLong()
    private val overlayInstances = AtomicLong()

    private data class Submission(
        val previous: ComposerDraft,
        val request: Long,
        val nativeDraft: SendDraftRef,
    )

    private data class ReplyResolution(
        val expectedId: String?,
        val target: ReplyTarget?,
    )

    private fun replyTarget(row: MessageRow) =
        ReplyTarget(
            row.id,
            row.sender,
            row.text.lineSequence().firstOrNull {
                it.isNotBlank()
            } ?: "Message",
        )

    private fun sendFailure(outcome: TextSendOutcome): AppFailure? =
        when (outcome) {
            is TextSendOutcome.Accepted -> {
                null
            }

            is TextSendOutcome.AcceptedPendingCommit -> {
                outcome.failure
            }

            is TextSendOutcome.Rejected -> {
                outcome.failure
            }

            is TextSendOutcome.Unknown -> {
                AppFailure(
                    "Send status is unknown. Check send recovery before you send again.",
                    "send-unknown",
                    FailureRecovery.OPEN_SETTINGS,
                )
            }
        }

    @Volatile private var conversation: Conversation? = null

    @Volatile private var boundKey: SessionKey? = null
    private var logicalKey: String? = null
    private var nextBefore: MessageHistoryPosition? = null
    private var recoveryUpper: MessageRecoveryPosition? = null
    private var recoveryNext: MessageRecoveryPosition? = null

    @Volatile private var foreground = false

    @Volatile private var atNewest = false

    @Volatile private var newestLoaded = false
    internal var lookupConversation: suspend (ActiveSession, String) -> Conversation? = conversations::find
    internal var actionMessageRead: suspend (ActiveSession, String) -> Message? = queries::message
    internal var historyPageRead: suspend (
        Conversation,
        ListMessagesOptions,
        MessageHistoryPosition?,
        MessageHistoryPosition?,
    ) -> MessageHistoryPage =
        queries::history
    internal var recoveryRead: suspend (
        Conversation,
        ListMessagesOptions,
        MessageRecoveryPosition?,
        MessageRecoveryPosition?,
    ) -> MessageRecoveryPage =
        queries::recovery
    internal var onOpenStarted: (Long, String) -> Unit = { _, _ -> }
    internal var onOpenAttemptFinished: (Long, String) -> Unit = { _, _ -> }
    internal var onOpenFinished: (String) -> Unit = {}
    internal var beforeQueuedAction: suspend (ChatAction) -> Unit = {}
    internal var onQueuedActionFinished: (ChatAction) -> Unit = {}

    fun clear() {
        conversation =
            null
        boundKey =
            null
        logicalKey =
            null
        nextBefore =
            null
        recoveryUpper =
            null
        recoveryNext =
            null
        atNewest = false
        newestLoaded = false
        cache.clear()
        synchronized(draftLock) { composerDrafts.clear() }
        ui.value = ChatState()
    }

    private fun routeConversationId(): String? =
        when (
            val route =
                destinations.state.value.route
        ) {
            is AppRoute.Chat -> route.conversationId
            is AppRoute.ConversationDetails -> route.conversationId
            is AppRoute.GroupFields -> route.conversationId
            is AppRoute.OwnFields -> route.conversationId
            else -> null
        }

    override fun currentConversation(): Conversation? {
        val key =
            boundKey ?: return null
        return conversation.takeIf {
            key == session.active.value?.key &&
                session.accepts(key) &&
                it?.id() == routeConversationId()
        }
    }

    fun foreground(value: Boolean) {
        foreground =
            value
        if (value) {
            val owner =
                session.active.value ?: return
            owner.work.launch {
                markRead(
                    owner,
                    destinations.state.value.token,
                )
            }
        }
    }

    fun dismissTopSurface(): Boolean {
        val overlay = ui.value.overlay ?: return false
        if (overlay.surface != MessageSurface.POPOVER) {
            return closeSheet(overlay.messageId, overlay.surface, overlay.instanceId)
        }
        var dismissed = false
        editCurrentChat { _, _ ->
            ui.update { current ->
                val shown = current.overlay
                if (shown != null &&
                    shown.instanceId == overlay.instanceId &&
                    shown.messageId == overlay.messageId &&
                    shown.surface == MessageSurface.POPOVER
                ) {
                    dismissed = true
                    current.copy(overlay = null)
                } else {
                    current
                }
            }
        }
        return dismissed
    }

    private fun closeSheet(
        messageId: String,
        surface: MessageSurface,
        instanceId: Long,
    ): Boolean {
        if (surface != MessageSurface.EMOJI && surface != MessageSurface.REACTORS) return false
        var closed = false
        editCurrentChat { _, _ ->
            ui.update { current ->
                val overlay = current.overlay
                if (overlay != null &&
                    overlay.messageId == messageId &&
                    overlay.surface == surface &&
                    overlay.instanceId == instanceId
                ) {
                    closed = true
                    current.copy(overlay = overlay.copy(surface = MessageSurface.POPOVER))
                } else {
                    current
                }
            }
        }
        return closed
    }

    fun destinationChanged() {
        if (destinations.state.value.route !is AppRoute.Chat ||
            ui.value.conversationId != routeConversationId()
        ) {
            ui.update {
                it.copy(
                    overlay =
                    null,
                )
            }
        }
    }

    fun screenToken() = destinations.state.value.token

    fun acceptsScreen(
        key: SessionKey,
        token: Long,
    ) = destinations.accepts(DestinationOrigin(key, token))

    override fun openConversation(id: String) {
        destinations.navigate(AppRoute.Chat(id))
    }

    private fun valid(
        owner: ActiveSession,
        token: Long,
    ) = acceptsScreen(owner.key, token)

    private fun <T> onCurrentScreen(
        owner: ActiveSession,
        token: Long,
        change: () -> T,
    ): T? {
        var result: T? = null
        destinations.admit(DestinationOrigin(owner.key, token)) { result = change() }
        return result
    }

    private fun requireOrigin(
        owner: ActiveSession,
        origin: DestinationOrigin,
        chat: Conversation,
    ) {
        if (boundKey != owner.key ||
            !destinations.accepts(origin) ||
            chat.id() != conversation?.id() ||
            destinations.state.value.route != AppRoute.Chat(chat.id())
        ) {
            throw CancellationException("Chat changed")
        }
    }

    private fun editCurrentChat(change: (ActiveSession, String) -> Unit): Boolean {
        val owner = session.active.value ?: return false
        val chat = conversation ?: return false
        val origin = destinations.capture(owner)
        var applied = false
        destinations.admit(origin) {
            if (boundKey == owner.key &&
                destinations.state.value.route == AppRoute.Chat(chat.id())
            ) {
                change(
                    owner,
                    chat.id(),
                )
                applied =
                    true
            }
        }
        return applied
    }

    suspend fun open(id: String) {
        val owner = session.active.value ?: return
        val origin = destinations.capture(owner)
        val attempt = openAttempts.incrementAndGet()
        onOpenStarted(attempt, id)
        try {
            if (!destinations.accepts(origin)) return
            val chat = lookupConversation(owner, id) ?: throw FeatureInputFailure("This chat is not available.")
            if (!destinations.accepts(origin)) return
            val group = reads.withPermit { (chat as? Conversation.Group)?.let { conversations.groupState(it.group) } }
            val common = group?.common ?: conversations.state(chat)
            val memberCount = reads.withPermit { chat.members().size }
            if (!destinations.accepts(origin)) return
            if (common.consentState == ConsentState.DENIED) throw FeatureInputFailure("This chat is blocked.")
            val logical = logicalConversationKey(chat, owner.client.inboxId())
            val anchor = session.preferences.anchor(owner.key.profileId, logical)
            val stored =
                synchronized(draftLock) {
                    composerDrafts[owner.key.profileId to id]
                } ?: drafts.read(
                    owner.key.profileId,
                    id,
                )
            if (!destinations.accepts(origin)) return
            projection.withLock {
                if (!destinations.accepts(origin)) return@withLock
                val same = conversation?.id() == id
                conversation = chat
                boundKey = owner.key
                logicalKey = logical
                val latestDraft =
                    synchronized(draftLock) {
                        composerDrafts.getOrPut(owner.key.profileId to id) {
                            stored
                        }
                    }
                val title = lists.row(id)?.title ?: group?.name?.ifBlank { "Group" } ?: id.take(12)
                lists.prepareRequest(owner, id, title)
                if (!same) {
                    atNewest = false
                    newestLoaded = false
                    nextBefore = null
                    recoveryUpper = null
                    recoveryNext = null
                    ui.value =
                        ChatState(
                            conversationId =
                            id,
                            title =
                            title,
                            group =
                                chat is Conversation.Group,
                            memberCount =
                            memberCount,
                            unknown =
                                common.consentState == ConsentState.UNKNOWN,
                            composer =
                                composer(latestDraft),
                            anchor =
                            anchor,
                        )
                } else {
                    ui.update {
                        it.copy(
                            title =
                            title,
                            memberCount =
                            memberCount,
                            unknown =
                                common.consentState == ConsentState.UNKNOWN,
                            failure =
                            null,
                        )
                    }
                }
                lists.bindRequest(owner, chat, title)
                val cached = cache.get(id)
                if (!same && cached != null && anchor != null && cached.rows.any { it.id == anchor.messageId }) {
                    nextBefore = cached.last
                    ui.update {
                        it.copy(
                            messages =
                                cached.rows.map { message ->
                                    message.toRow(owner.client.inboxId())
                                },
                            anchor =
                            anchor,
                        )
                    }
                }
                sends.recoverAccepted(owner.key)
                recoverSubmission(owner, id)
                refreshTimeline(owner, origin.destinationToken, preserve = same || anchor?.wasAtNewest == false)
            }
        } catch (error: Throwable) {
            if (error is CancellationException) throw error
            if (destinations.accepts(origin)) {
                ui.update {
                    it.copy(
                        failure =
                            featureFailure(
                                error,
                                "Cannot open this chat. Try again.",
                                "chat-open",
                            ),
                    )
                }
            }
        } finally {
            onOpenAttemptFinished(attempt, id)
            onOpenFinished(id)
        }
    }

    private fun composer(draft: ComposerDraft): ComposerState =
        ComposerState(
            text =
                draft.text,
            selectionStart =
                draft.text.length,
            selectionEnd =
                draft.text.length,
            revision =
                draft.revision,
            reply =
                draft.replyId?.let {
                    ReplyTarget(
                        it,
                        "",
                        "",
                    )
                },
            pendingRequestId =
                draft.requestId,
            submittedRevision =
                draft.submittedRevision,
            enabled =
                draft.draftId == null,
            result =
                draft.requestId?.let { request ->
                    draft.acceptedId?.let {
                        TextSendResult(
                            request,
                            conversation?.id().orEmpty(),
                            draft.submittedRevision ?: draft.revision,
                            TextSendOutcome.AcceptedPendingCommit(
                                it,
                                AppFailure(
                                    "Send accepted. Local recovery is pending.",
                                    "accepted-commit",
                                ),
                            ),
                        )
                    } ?: draft.draftId?.let {
                        TextSendResult(
                            request,
                            conversation?.id().orEmpty(),
                            draft.submittedRevision ?: draft.revision,
                            TextSendOutcome.Unknown(it),
                        )
                    }
                },
        )

    private fun committedComposer(
        draft: ComposerDraft,
        result: TextSendResult,
    ): ComposerState {
        val displayed = ui.value.composer
        val base =
            if (displayed.text == draft.text &&
                displayed.revision == draft.revision
            ) {
                displayed
            } else {
                composer(draft)
            }
        return base.copy(
            reply =
                displayed.reply.takeIf {
                    it?.messageId == draft.replyId
                } ?: base.reply,
            pendingRequestId =
            null,
            submittedRevision =
            null,
            enabled =
            true,
            result =
            result,
        )
    }

    fun dispatch(action: ChatAction) {
        when (action) {
            is ChatAction.ChangeDraft -> {
                changeDraft(action)
                return
            }

            is ChatAction.Reply -> {
                reply(action.messageId)
                return
            }

            is ChatAction.SelectMessage -> {
                editCurrentChat {
                    _,
                    _,
                    ->
                    ui.update {
                        it.copy(
                            overlay =
                                MessageOverlayState(action.messageId, instanceId = overlayInstances.incrementAndGet()),
                        )
                    }
                }
                return
            }

            is ChatAction.ShowEmoji -> {
                editCurrentChat {
                    _,
                    _,
                    ->
                    ui.update {
                        it.copy(
                            overlay =
                                MessageOverlayState(
                                    action.messageId,
                                    MessageSurface.EMOJI,
                                    instanceId = overlayInstances.incrementAndGet(),
                                ),
                        )
                    }
                }
                return
            }

            is ChatAction.ShowReactors -> {
                editCurrentChat {
                    _,
                    _,
                    ->
                    ui.update {
                        it.copy(
                            overlay =
                                MessageOverlayState(
                                    action.messageId,
                                    MessageSurface.REACTORS,
                                    emoji =
                                        action.emoji,
                                    instanceId = overlayInstances.incrementAndGet(),
                                ),
                        )
                    }
                }
                return
            }

            is ChatAction.ChangeEmojiQuery -> {
                editCurrentChat {
                    _,
                    _,
                    ->
                    ui.update {
                        it.copy(
                            overlay =
                                it.overlay?.copy(
                                    emojiQuery =
                                        action.value,
                                ),
                        )
                    }
                }
                return
            }

            is ChatAction.CloseSheet -> {
                closeSheet(action.messageId, action.surface, action.instanceId)
                return
            }

            ChatAction.DismissOverlay -> {
                editCurrentChat { _, _ -> ui.update { it.copy(overlay = null) } }
                return
            }

            is ChatAction.Consent -> {
                editCurrentChat { _, _ -> lists.consent(action.allowed) }
                return
            }

            ChatAction.SelectAttachment, is ChatAction.OpenAttachment -> {
                return
            }

            else -> {
                Unit
            }
        }
        val owner = session.active.value ?: return
        val chat = conversation ?: return
        val origin = destinations.capture(owner)
        val submission = if (action is ChatAction.SendText) reserve(owner, origin, chat, action) ?: return else null
        owner.work.launch(Dispatchers.IO) {
            var sendStarted = false
            try {
                beforeQueuedAction(action)
                requireOrigin(owner, origin, chat)
                when (action) {
                    is ChatAction.SendText -> {
                        sendStarted =
                            true
                        send(
                            owner,
                            origin,
                            chat,
                            action,
                            checkNotNull(submission),
                        )
                    }

                    is ChatAction.RetrySend -> {
                        sends.retry(
                            owner.key,
                            owner.client,
                            chat,
                            action.messageId,
                            admission =
                                {
                                    destinations.accepts(origin) &&
                                        conversation?.id() == chat.id() &&
                                        destinations.state.value.route == AppRoute.Chat(chat.id())
                                },
                        ) {
                            reconcileFeatureMessage(
                                owner,
                                origin.destinationToken,
                                it,
                            )
                        }
                        if (session.accepts(owner.key)) recoverSubmission(owner, chat.id())
                        onCurrentScreen(
                            owner,
                            origin.destinationToken,
                        ) {
                            if (ui.value.failure?.code == "send-publication") {
                                ui.update {
                                    it.copy(
                                        failure =
                                        null,
                                    )
                                }
                            }
                        }
                    }

                    is ChatAction.React -> {
                        val message =
                            actionMessageRead(
                                owner,
                                action.messageId,
                            ) ?: throw FeatureInputFailure("This message is not available.")
                        requireOrigin(owner, origin, chat)
                        if (message.conversationId != chat.id()) {
                            throw FeatureInputFailure("This message belongs to another chat.")
                        }
                        sends.queue(
                            owner.key,
                            owner.client,
                            chat,
                            admission =
                                {
                                    destinations.accepts(origin) &&
                                        conversation?.id() == chat.id()
                                },
                            reconcile =
                                {
                                    reconcileFeatureMessage(
                                        owner,
                                        origin.destinationToken,
                                        it,
                                    )
                                },
                        ) {
                            requireOrigin(owner, origin, chat)
                            message.react(
                                Reaction(
                                    action.emoji,
                                    if (action.remove) ReactionAction.REMOVED else ReactionAction.ADDED,
                                    ReactionSchema.UNICODE,
                                ),
                                SendOptions(
                                    optimistic =
                                    true,
                                ),
                            )
                        }
                        refresh(owner)
                    }

                    is ChatAction.DeleteMessage -> {
                        val message =
                            actionMessageRead(
                                owner,
                                action.messageId,
                            ) ?: throw FeatureInputFailure("This message is not available.")
                        requireOrigin(owner, origin, chat)
                        if (message.conversationId != chat.id()) {
                            throw FeatureInputFailure("This message belongs to another chat.")
                        }
                        chat.deleteMessage(action.messageId)
                        refresh(owner)
                    }

                    is ChatAction.EditFailedSend -> {
                        val row =
                            ui.value.messages.firstOrNull {
                                it.id == action.messageId &&
                                    it.mine &&
                                    it.sendState == MessageSendState.NOT_SENT
                            } ?: return@launch
                        changeDraft(ChatAction.ChangeDraft(row.text, row.text.length, row.text.length))
                    }

                    is ChatAction.Viewport -> {
                        saveViewport(owner, origin.destinationToken, action)
                    }

                    ChatAction.LoadOlder -> {
                        projection.withLock { loadOlder(owner, origin.destinationToken) }
                    }

                    ChatAction.JumpToLatest -> {
                        projection.withLock {
                            refreshTimeline(
                                owner,
                                origin.destinationToken,
                                false,
                            )
                        }
                    }

                    ChatAction.LoadOlderRecovery,
                    ChatAction.LatestRecovery,
                    -> {
                        projection.withLock {
                            if (destinations.accepts(origin)) {
                                recoveryUpper =
                                    if (action == ChatAction.LatestRecovery) null else recoveryNext
                                refreshTimeline(
                                    owner,
                                    origin.destinationToken,
                                    true,
                                )
                            }
                        }
                    }

                    ChatAction.Refresh -> {
                        refresh(owner)
                    }

                    else -> {
                        Unit
                    }
                }
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                if (destinations.accepts(origin)) {
                    ui.update {
                        it.copy(
                            failure =
                                featureFailure(
                                    error,
                                    "Cannot complete this message action. Try again.",
                                    "chat-action",
                                ),
                        )
                    }
                }
            } finally {
                if (submission != null &&
                    !sendStarted
                ) {
                    releaseReservation(
                        owner,
                        chat.id(),
                        submission,
                    )
                }
                onQueuedActionFinished(action)
            }
        }
    }

    private fun currentDraft(
        owner: ActiveSession,
        id: String,
    ) = synchronized(draftLock) {
        composerDrafts[owner.key.profileId to id] ?: ComposerDraft()
    }

    /** Acquire the session fence, draft lock, and secure store lock in this order. */
    private fun admittedDraft(
        owner: ActiveSession,
        change: () -> Unit,
    ): Boolean =
        session.admit(owner.key) {
            synchronized(draftLock) {
                change()
            }
        }

    private fun persist(
        owner: ActiveSession,
        id: String,
        value: ComposerDraft,
    ) {
        scope.launch(Dispatchers.IO) {
            try {
                admittedDraft(owner) {
                    if (composerDrafts[owner.key.profileId to id] == value) {
                        drafts.write(
                            owner.key,
                            id,
                            value,
                        )
                    }
                }
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                session.withCurrent(owner.key) {
                    if (ui.value.conversationId == id) {
                        ui.update {
                            it.copy(
                                failure =
                                    featureFailure(
                                        error,
                                        "Cannot save this draft. Keep the app open and try again.",
                                        "composer-store",
                                    ),
                            )
                        }
                    }
                }
            }
        }
    }

    private fun changeDraft(action: ChatAction.ChangeDraft) =
        editCurrentChat { owner, id ->
            val next =
                synchronized(draftLock) {
                    val old = currentDraft(owner, id)
                    old
                        .copy(
                            text =
                                action.text,
                            revision =
                                if (old.text == action.text) old.revision else old.revision + 1,
                        ).also {
                            composerDrafts[owner.key.profileId to id] =
                                it
                        }
                }
            ui.update {
                it.copy(
                    composer =
                        it.composer.copy(
                            text =
                                action.text,
                            selectionStart =
                                action.selectionStart,
                            selectionEnd =
                                action.selectionEnd,
                            compositionStart =
                                action.compositionStart,
                            compositionEnd =
                                action.compositionEnd,
                            revision =
                                next.revision,
                        ),
                )
            }
            persist(owner, id, next)
        }

    private fun reply(id: String?) =
        editCurrentChat { owner, conversationId ->
            val parent = ui.value.messages.firstOrNull { it.id == id && !it.deleted }
            val next =
                synchronized(draftLock) {
                    currentDraft(
                        owner,
                        conversationId,
                    ).copy(
                        replyId =
                            parent?.id,
                    ).also {
                        composerDrafts[owner.key.profileId to conversationId] =
                            it
                    }
                }
            ui.update {
                it.copy(
                    composer = it.composer.copy(revision = next.revision, reply = parent?.let(::replyTarget)),
                )
            }
            persist(owner, conversationId, next)
        }

    private fun reserve(
        owner: ActiveSession,
        origin: DestinationOrigin,
        chat: Conversation,
        action: ChatAction.SendText,
    ): Submission? {
        var result: Submission? = null
        destinations.admit(origin) {
            if (boundKey != owner.key || destinations.state.value.route != AppRoute.Chat(chat.id())) return@admit
            synchronized(draftLock) {
                val previous =
                    currentDraft(owner, chat.id()).copy(
                        replyId =
                            ui.value.composer.reply
                                ?.messageId,
                    )
                if (previous.draftId != null ||
                    ui.value.composer.pendingRequestId != null ||
                    action.text.isBlank() ||
                    action.submittedRevision != previous.revision ||
                    action.text != previous.text
                ) {
                    return@synchronized
                }
                val request = action.requestId.takeIf { it != 0L } ?: requests.incrementAndGet()
                val nativeDraft = SendDraftRef(UUID.randomUUID().toString(), chat.id())
                composerDrafts[owner.key.profileId to chat.id()] =
                    previous.copy(
                        requestId =
                        request,
                        submittedRevision =
                            previous.revision,
                        draftId =
                            nativeDraft.draftId,
                        submittedReplyId =
                            previous.replyId,
                    )
                ui.update {
                    it.copy(
                        composer =
                            it.composer.copy(
                                pendingRequestId =
                                request,
                                submittedRevision =
                                    previous.revision,
                                enabled =
                                false,
                                result =
                                null,
                            ),
                        failure =
                        null,
                    )
                }
                result = Submission(previous, request, nativeDraft)
            }
        }
        return result
    }

    private fun releaseReservation(
        owner: ActiveSession,
        id: String,
        submission: Submission,
    ) {
        if (!session.accepts(owner.key)) return
        admittedDraft(owner) {
            val current = currentDraft(owner, id)
            if (current.draftId != submission.nativeDraft.draftId || current.acceptedId != null) return@admittedDraft
            val next =
                current.copy(
                    requestId =
                    null,
                    submittedRevision =
                    null,
                    draftId =
                    null,
                    acceptedId =
                    null,
                    submittedReplyId =
                    null,
                )
            composerDrafts[owner.key.profileId to id] = next
            if (ui.value.conversationId == id) ui.update { it.copy(composer = composer(next)) }
            try {
                drafts.write(
                    owner.key,
                    id,
                    next,
                )
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
            }
        }
    }

    private suspend fun send(
        owner: ActiveSession,
        origin: DestinationOrigin,
        chat: Conversation,
        action: ChatAction.SendText,
        submission: Submission,
    ) {
        val id = chat.id()
        val previous = submission.previous
        val request = submission.request
        val nativeDraft = submission.nativeDraft
        var acceptedId: String? = null
        var committed = false
        try {
            if (!admittedDraft(owner) {
                    drafts.write(
                        owner.key,
                        id,
                        currentDraft(
                            owner,
                            id,
                        ),
                    )
                }
            ) {
                throw CancellationException("Session changed before draft commit")
            }
            requireOrigin(owner, origin, chat)
            val parent =
                previous.replyId?.let { messageId ->
                    actionMessageRead(
                        owner,
                        messageId,
                    ).also {
                        requireOrigin(
                            owner,
                            origin,
                            chat,
                        )
                        if (it == null ||
                            it.conversationId != id
                        ) {
                            throw FeatureInputFailure("The reply message is not available.")
                        }
                    }
                }
            sends.queue(
                owner.key,
                owner.client,
                chat,
                draft =
                nativeDraft,
                admission =
                    {
                        destinations.accepts(origin) &&
                            conversation?.id() == id
                    },
                onStored = { messageId ->
                    acceptedId = messageId
                    if (ui.value.conversationId == id &&
                        session.accepts(owner.key)
                    ) {
                        ui.update {
                            val outcome =
                                TextSendOutcome.AcceptedPendingCommit(
                                    messageId,
                                    AppFailure(
                                        "Send accepted. Local recovery is pending.",
                                        "accepted-commit",
                                        FailureRecovery.RETRY,
                                    ),
                                )
                            it.copy(
                                composer =
                                    it.composer.copy(
                                        result =
                                            TextSendResult(
                                                request,
                                                id,
                                                previous.revision,
                                                outcome,
                                            ),
                                    ),
                                failure =
                                    sendFailure(outcome),
                            )
                        }
                    }
                    admittedDraft(owner) {
                        val current = currentDraft(owner, id).copy(acceptedId = messageId)
                        composerDrafts[owner.key.profileId to id] = current
                        drafts.write(owner.key, id, current)
                    }
                },
                onAccepted = { messageId ->
                    admittedDraft(owner) {
                        val current = currentDraft(owner, id)
                        val matchesRequest = current.requestId == request
                        val matchesRevision = current.submittedRevision == previous.revision
                        if (matchesRequest && matchesRevision) {
                            val next =
                                if (current.revision == previous.revision) {
                                    ComposerDraft(
                                        revision =
                                            current.revision + 1,
                                        replyId =
                                            current.replyId.takeUnless {
                                                it == previous.replyId
                                            },
                                    )
                                } else {
                                    current.copy(
                                        requestId =
                                        null,
                                        submittedRevision =
                                        null,
                                        draftId =
                                        null,
                                        acceptedId =
                                        null,
                                        submittedReplyId =
                                        null,
                                    )
                                }
                            if (drafts.write(owner.key, id, next)) {
                                composerDrafts[owner.key.profileId to id] = next
                                committed = true
                                if (ui.value.conversationId == id &&
                                    session.accepts(owner.key)
                                ) {
                                    ui.update {
                                        it.copy(
                                            composer =
                                                committedComposer(
                                                    next,
                                                    TextSendResult(
                                                        request,
                                                        id,
                                                        previous.revision,
                                                        TextSendOutcome.Accepted(messageId),
                                                    ),
                                                ),
                                            failure =
                                            null,
                                        )
                                    }
                                }
                            }
                        }
                    }
                },
                reconcile = { reconcileFeatureMessage(owner, origin.destinationToken, it) },
            ) {
                requireOrigin(owner, origin, chat)
                if (parent != null) {
                    parent.reply(
                        action.text,
                        SendOptions(
                            optimistic =
                            true,
                        ),
                    )
                } else {
                    chat.sendText(
                        action.text,
                        SendOptions(
                            optimistic =
                            true,
                        ),
                    )
                }
            }
            if (!committed && session.accepts(owner.key)) {
                acceptedId?.let { messageId ->
                    if (ui.value.conversationId == id) {
                        ui.update {
                            val outcome =
                                TextSendOutcome.AcceptedPendingCommit(
                                    messageId,
                                    AppFailure(
                                        "Send accepted. Local recovery is pending.",
                                        "accepted-commit",
                                        FailureRecovery.RETRY,
                                    ),
                                )
                            it.copy(
                                composer =
                                    it.composer.copy(
                                        result =
                                            TextSendResult(
                                                request,
                                                id,
                                                previous.revision,
                                                outcome,
                                            ),
                                    ),
                                failure =
                                    sendFailure(outcome),
                            )
                        }
                    }
                }
            }
        } catch (error: Throwable) {
            if (committed) {
                if (error is CancellationException) throw error
                onCurrentScreen(
                    owner,
                    origin.destinationToken,
                ) {
                    ui.update {
                        it.copy(
                            failure =
                                featureFailure(
                                    error,
                                    "Message is saved but was not sent. Retry this message.",
                                    "send-publication",
                                ),
                        )
                    }
                }
                return
            }
            if (!session.accepts(owner.key)) {
                if (error is CancellationException) throw error
                return
            }
            val ref = session.preferences.drafts(owner.key.profileId).firstOrNull { it.draftId == nativeDraft.draftId }
            val accepted = acceptedId ?: ref?.acceptedMessageId
            val outcome =
                when {
                    accepted != null -> {
                        TextSendOutcome.AcceptedPendingCommit(
                            accepted,
                            AppFailure(
                                "Send accepted. Local recovery is pending.",
                                "accepted-commit",
                                FailureRecovery.RETRY,
                            ),
                        )
                    }

                    ref?.phase == SendPhase.QUEUEING -> {
                        TextSendOutcome.Unknown(nativeDraft.draftId)
                    }

                    else -> {
                        TextSendOutcome.Rejected(
                            if (error is CancellationException) {
                                AppFailure(
                                    "Send was cancelled. Your draft is kept.",
                                    "send-cancelled",
                                )
                            } else {
                                featureFailure(
                                    error,
                                    "Cannot send this message. Your draft is kept.",
                                    "send",
                                )
                            },
                        )
                    }
                }
            admittedDraft(owner) {
                val current = currentDraft(owner, id)
                val next =
                    if (outcome is TextSendOutcome.Rejected) {
                        current.copy(
                            requestId =
                            null,
                            submittedRevision =
                            null,
                            draftId =
                            null,
                            acceptedId =
                            null,
                            submittedReplyId =
                            null,
                        )
                    } else {
                        current.copy(
                            acceptedId =
                            accepted,
                        )
                    }
                composerDrafts[owner.key.profileId to id] = next
                if (ui.value.conversationId == id &&
                    session.accepts(owner.key)
                ) {
                    ui.update {
                        it.copy(
                            composer =
                                it.composer.copy(
                                    pendingRequestId =
                                        if (outcome is TextSendOutcome.Rejected) null else request,
                                    enabled =
                                        outcome is TextSendOutcome.Rejected,
                                    result =
                                        TextSendResult(
                                            request,
                                            id,
                                            previous.revision,
                                            outcome,
                                        ),
                                ),
                            failure =
                                sendFailure(outcome),
                        )
                    }
                }
                try {
                    drafts.write(
                        owner.key,
                        id,
                        next,
                    )
                } catch (storeError: Throwable) {
                    if (storeError is CancellationException) throw storeError
                }
            }
            if (error is CancellationException) throw error
        }
    }

    override suspend fun refresh() {
        session.active.value?.let { refresh(it) }
    }

    suspend fun refresh(owner: ActiveSession) {
        sends.recoverAccepted(owner.key)
        if (conversation == null || boundKey != owner.key || !session.accepts(owner.key)) return
        recoverSubmission(owner, checkNotNull(conversation).id())
        projection.withLock { refreshTimeline(owner, destinations.state.value.token, true) }
    }

    private suspend fun recoverSubmission(
        owner: ActiveSession,
        id: String,
    ) {
        val saved = currentDraft(owner, id)
        val draftId = saved.draftId ?: return
        val references = session.preferences.drafts(owner.key.profileId)
        val reference = references.firstOrNull { it.draftId == draftId }
        val acceptedId = saved.acceptedId ?: reference?.acceptedMessageId
        if (acceptedId != null) {
            val acceptedReference =
                SendDraftRef(
                    draftId,
                    id,
                    acceptedMessageId =
                    acceptedId,
                    phase =
                        SendPhase.ACCEPTED,
                )
            if (reference == null) {
                val native = queries.message(owner, acceptedId) ?: return
                if (!session.accepts(owner.key) ||
                    native.conversationId != id ||
                    native.senderInboxId != owner.client.inboxId()
                ) {
                    return
                }
                if (native.deliveryStatus != DeliveryStatus.PUBLISHED) {
                    session.preferences.saveDraft(
                        owner.key.profileId,
                        acceptedReference,
                    ) { change ->
                        session.admit(
                            owner.key,
                            change,
                        )
                    }
                    if (!session.preferences.drafts(owner.key.profileId).any {
                            it.draftId == draftId &&
                                it.phase == SendPhase.ACCEPTED &&
                                it.acceptedMessageId == acceptedId
                        }
                    ) {
                        return
                    }
                }
            } else if (!session.preferences.saveAcceptedDraft(owner.key.profileId, acceptedReference)) {
                return
            }
            admittedDraft(owner) {
                val current = currentDraft(owner, id)
                if (current.requestId != saved.requestId ||
                    current.submittedRevision != saved.submittedRevision
                ) {
                    return@admittedDraft
                }
                val next =
                    if (current.revision == saved.submittedRevision) {
                        ComposerDraft(
                            revision =
                                current.revision + 1,
                            replyId =
                                current.replyId.takeUnless {
                                    it == saved.submittedReplyId
                                },
                        )
                    } else {
                        current.copy(
                            requestId =
                            null,
                            submittedRevision =
                            null,
                            draftId =
                            null,
                            acceptedId =
                            null,
                            submittedReplyId =
                            null,
                        )
                    }
                if (drafts.write(owner.key, id, next)) {
                    composerDrafts[owner.key.profileId to id] = next
                    if (ui.value.conversationId == id) {
                        ui.update {
                            it.copy(
                                composer =
                                    committedComposer(
                                        next,
                                        TextSendResult(
                                            saved.requestId ?: 0,
                                            id,
                                            saved.submittedRevision ?: saved.revision,
                                            TextSendOutcome.Accepted(acceptedId),
                                        ),
                                    ),
                                failure =
                                null,
                            )
                        }
                    }
                }
            }
        } else if (reference == null && !sends.isInFlight(draftId)) {
            admittedDraft(owner) {
                val current = currentDraft(owner, id)
                val next =
                    current.copy(
                        requestId =
                        null,
                        submittedRevision =
                        null,
                        draftId =
                        null,
                        acceptedId =
                        null,
                        submittedReplyId =
                        null,
                    )
                if (drafts.write(
                        owner.key,
                        id,
                        next,
                    )
                ) {
                    composerDrafts[owner.key.profileId to id] =
                        next
                    if (ui.value.conversationId == id) {
                        ui.update {
                            it.copy(
                                composer =
                                    composer(next),
                            )
                        }
                    }
                }
            }
        }
    }

    suspend fun onMessage(
        owner: ActiveSession,
        message: Message,
    ) {
        if (message.conversationId == conversation?.id()) refresh(owner)
    }

    override suspend fun reconcileFeatureMessage(
        owner: ActiveSession,
        token: Long,
        message: Message,
    ) {
        if (valid(
                owner,
                token,
            ) &&
            message.conversationId == conversation?.id()
        ) {
            projection.withLock {
                refreshTimeline(
                    owner,
                    token,
                    true,
                )
            }
        }
    }

    private fun historyPages(chat: Conversation) =
        SDKHistoryPages<Message>(
            readableRow = {
                (it.content as? SDKMessageContent.Standard)?.value !is MessageContent.DeletedMessage
            },
        ) { direction, before, after ->
            reads.withPermit {
                historyPageRead(
                    chat,
                    publishedSelection().copy(limit = 50u, direction = direction),
                    before,
                    after,
                ).queryPage()
            }
        }

    private suspend fun overlay(chat: Conversation): PendingMessagePage =
        pendingMessagePage(recoveryUpper) { options, before, after -> recoveryRead(chat, options, before, after) }

    private fun ChatState.withRecovery(page: PendingMessagePage): ChatState {
        recoveryNext = page.last
        return copy(
            hasOlderRecovery = page.hasOlder,
            recoveryAtNewest = recoveryUpper == null,
            recoveryNotice = page.notice,
        )
    }

    private fun timelineRows(
        owner: ActiveSession,
        published: List<Message>,
        queued: List<Message>,
    ) = (published + queued)
        .distinctBy { it.id }
        .sortedByDescending { it.sentAt.ns }
        .map { it.toRow(owner.client.inboxId()) }

    private fun anchorForWindow(
        owner: ActiveSession,
        saved: ScrollAnchor?,
        window: HistoryWindow<Message>,
    ): RestoredPosition {
        val readable = { row: Message -> !row.toRow(owner.client.inboxId()).deleted }
        if (saved == null || saved.wasAtNewest || window.changed) {
            val row = window.rows.firstOrNull(readable)
            return RestoredPosition(
                row?.let {
                    ScrollAnchor(it.id, it.sentAt.ns, 0, window.atNewest, it.deliveryCursor)
                },
                window.changed,
            )
        }
        val match = window.rows.firstOrNull { it.id == saved.messageId && readable(it) }
        if (match != null) {
            return RestoredPosition(
                saved.copy(sentAtNs = match.sentAt.ns, deliveryCursor = match.deliveryCursor),
                false,
            )
        }
        val replacement =
            window.rows.take(window.newerCount).lastOrNull(readable)
                ?: window.rows.drop(window.newerCount).firstOrNull(readable)
        return RestoredPosition(
            replacement?.let { ScrollAnchor(it.id, it.sentAt.ns, 0, false, it.deliveryCursor) },
            true,
        )
    }

    private suspend fun refreshTimeline(
        owner: ActiveSession,
        token: Long,
        preserve: Boolean,
    ) {
        val saved = ui.value.anchor.takeIf { preserve && it?.wasAtNewest == false }
        refreshWindow(owner, token, saved)
    }

    private suspend fun refreshWindow(
        owner: ActiveSession,
        token: Long,
        saved: ScrollAnchor?,
    ) {
        val chat = conversation ?: return
        var boundary = saved?.deliveryCursor?.let { MessageHistoryPosition(Timestamp(saved.sentAtNs), it) }
        if (saved != null && boundary == null) {
            // Migrate only a readable old anchor. A deleted row cannot supply a boundary.
            val row = queries.message(owner, saved.messageId)
            if (row != null && !row.toRow(owner.client.inboxId()).deleted) boundary = row.historyPosition()
        }
        if (!valid(owner, token)) return
        var changed = saved != null && boundary == null
        val result =
            try {
                val pages = historyPages(chat)
                if (boundary == null) pages.older() else pages.around(boundary)
            } catch (error: XmtpException.InvalidArgument) {
                if (boundary == null) throw error
                val key = logicalKey ?: return
                session.preferences.clearAnchor(
                    owner.key.profileId,
                    key,
                ) { change -> admitPosition(owner, token, false, change) }
                if (!valid(owner, token)) return
                changed = true
                historyPages(chat).older()
            }
        val queued = overlay(chat)
        val consent = conversations.state(chat).consentState
        val reply =
            resolveReply(
                owner,
                chat,
                token,
                (result.rows + queued.rows).map {
                    it.toRow(owner.client.inboxId())
                },
            )
        if (!valid(owner, token) || chat.id() != conversation?.id()) return
        onCurrentScreen(owner, token) {
            val window =
                if (saved != null && !changed) {
                    cache.refresh(chat.id(), result, saved.messageId)
                } else {
                    cache.put(chat.id(), result, saved?.messageId)
                }
            nextBefore = window.last
            newestLoaded = window.atNewest
            val position = anchorForWindow(owner, saved, window)
            ui.update { currentUi ->
                currentUi
                    .copy(
                        messages = timelineRows(owner, window.rows, queued.rows),
                        historyNotice = window.notice ?: if (changed || position.changed) "Position changed" else null,
                        hasOlder = window.hasOlder,
                        unknown = consent == ConsentState.UNKNOWN,
                        anchor = position.anchor,
                    ).withRecovery(queued)
                    .refreshReply(reply)
            }
        }
        markRead(owner, token)
    }

    private suspend fun loadOlder(
        owner: ActiveSession,
        token: Long,
    ) {
        if (!ui.value.hasOlder) return
        val chat = conversation ?: return
        val page = historyPages(chat).older(nextBefore)
        val queued = overlay(chat)
        val reply = resolveReply(owner, chat, token, (page.rows + queued.rows).map { it.toRow(owner.client.inboxId()) })
        if (!valid(owner, token) || chat.id() != conversation?.id()) return
        onCurrentScreen(owner, token) {
            val window = cache.append(chat.id(), page, ui.value.anchor?.messageId)
            nextBefore = window.last
            newestLoaded = window.atNewest
            ui.update { currentUi ->
                currentUi
                    .copy(
                        messages = timelineRows(owner, window.rows, queued.rows),
                        historyNotice = window.notice,
                        hasOlder = window.hasOlder,
                    ).withRecovery(queued)
                    .refreshReply(reply)
            }
        }
    }

    private suspend fun restorePosition(
        owner: ActiveSession,
        token: Long,
        saved: ScrollAnchor,
    ) = refreshWindow(owner, token, saved)

    private suspend fun resolveReply(
        owner: ActiveSession,
        chat: Conversation,
        token: Long,
        rows: List<MessageRow>,
    ): ReplyResolution {
        val id = currentDraft(owner, chat.id()).replyId ?: return ReplyResolution(null, null)
        val loaded = rows.firstOrNull { it.id == id }
        val row =
            loaded ?: reads.withPermit {
                val native = queries.message(owner, id)
                if (native != null &&
                    native.conversationId != chat.id()
                ) {
                    throw FeatureInputFailure("The reply message belongs to another chat.")
                }
                native?.toRow(owner.client.inboxId())
            }
        if (!valid(
                owner,
                token,
            ) ||
            conversation?.id() != chat.id()
        ) {
            throw CancellationException("Reply destination changed")
        }
        if (row != null && !row.deleted) return ReplyResolution(id, replyTarget(row))
        admittedDraft(owner) {
            val current = currentDraft(owner, chat.id())
            if (current.replyId == id) {
                val next = current.copy(replyId = null)
                composerDrafts[owner.key.profileId to chat.id()] = next
                drafts.write(owner.key, chat.id(), next)
            }
        }
        return ReplyResolution(id, null)
    }

    private fun ChatState.refreshReply(resolution: ReplyResolution): ChatState {
        if (composer.reply?.messageId != resolution.expectedId) return this
        return copy(composer = composer.copy(reply = resolution.target))
    }

    private suspend fun saveViewport(
        owner: ActiveSession,
        token: Long,
        action: ChatAction.Viewport,
    ) = projection.withLock {
        if (!valid(
                owner,
                token,
            ) || destinations.state.value.route != AppRoute.Chat(conversation?.id().orEmpty())
        ) {
            return
        }
        if (!admitPosition(
                owner,
                token,
                false,
            ) {
                atNewest =
                    action.atNewest
                ui.update {
                    it.copy(
                        atNewest =
                            action.atNewest,
                    )
                }
            }
        ) {
            return
        }
        val key = logicalKey ?: return
        val published =
            conversation
                ?.id()
                ?.let { cache.get(it) }
                ?.rows
                ?.firstOrNull { it.id == action.anchor.messageId } ?: return@withLock
        val cursor = published.deliveryCursor ?: return@withLock
        val anchor = action.anchor.copy(sentAtNs = published.sentAt.ns, deliveryCursor = cursor)
        session.preferences
            .saveAnchor(
                owner.key.profileId,
                key,
                anchor,
                admit = { change -> admitPosition(owner, token, false, change) },
            )
        if (valid(
                owner,
                token,
            )
        ) {
            onCurrentScreen(owner, token) {
                ui.update { currentUi -> currentUi.copy(anchor = anchor) }
            }
            markRead(
                owner,
                token,
            )
        }
    }

    private suspend fun markRead(
        owner: ActiveSession,
        token: Long,
    ) {
        val chat = conversation ?: return
        val key = logicalKey ?: return
        markLocalRead(
            eligible = {
                foreground && atNewest && newestLoaded &&
                    valid(
                        owner,
                        token,
                    ) && destinations.state.value.route == AppRoute.Chat(chat.id())
            },
            latestIncoming = {
                queries
                    .messages(
                        chat,
                        incomingSelection(
                            owner.client
                                .inboxId(),
                        ).copy(
                            sortBy =
                                MessageSortBy.INSERTED_AT,
                            limit = 1u,
                        ),
                    ).firstOrNull()
                    ?.insertedAt
                    ?.ns
            },
            currentMarker = {
                session.preferences
                    .marker(
                        owner.key.profileId,
                        key,
                    ).insertedAtNs
            },
            saveMarker = {
                session.preferences
                    .saveMarker(
                        owner.key.profileId,
                        key,
                        it,
                        admit = { change -> admitPosition(owner, token, true, change) },
                    )
                if (valid(owner, token)) lists.refreshUnread(owner, chat, key)
            },
        )
    }

    private fun admitPosition(
        owner: ActiveSession,
        token: Long,
        read: Boolean,
        change: () -> Unit,
    ): Boolean {
        var applied = false
        destinations.admit(DestinationOrigin(owner.key, token)) {
            if (destinations.state.value.route == AppRoute.Chat(conversation?.id().orEmpty()) &&
                (
                    !read ||
                        (
                            foreground &&
                                atNewest &&
                                newestLoaded
                        )
                )
            ) {
                change()
                applied =
                    true
            }
        }
        return applied
    }

    internal suspend fun performancePage(
        chat: Conversation,
        before: MessageHistoryPosition?,
    ) = historyPages(chat).older(before)

    internal fun performanceRetain(
        id: String,
        page: HistoryWindow<Message>,
    ): List<Message> =
        cache
            .append(
                id,
                page,
                null,
            ).rows

    internal fun performanceCacheRows(ids: List<String>) = ids.sumOf { cache.get(it)?.rows?.size ?: 0 }

    internal fun performanceClearCache() = cache.clear()
}
