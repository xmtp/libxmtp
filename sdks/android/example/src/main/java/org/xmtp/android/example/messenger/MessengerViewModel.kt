package org.xmtp.android.example.messenger
import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.sync.withPermit
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.ExampleApp
import org.xmtp.android.example.shared.*
import uniffi.xmtp_sdk.*
import java.util.UUID
import java.util.concurrent.atomic.AtomicLong

class MessengerViewModel(
    application: Application,
) : AndroidViewModel(application) {
    val session =
        (application as ExampleApp).session
    private val ui =
        MutableStateFlow(
            MessengerState(
                backend =
                    BuildConfig.XMTP_BACKEND_URL,
            ),
        )
    val state: StateFlow<MessengerState> = ui
    val sends =
        SendCoordinator(
            session.preferences,
            session::accepts,
            session::admit,
        )
    private val reads = Semaphore(4)
    private val projection = Mutex()
    private val cache =
        TranscriptCache<Message>(
            {
                it.id
            },
            {
                it.sentAt.ns
            },
        )
    private val screenCounter = AtomicLong()
    private val screenGeneration get() =
        screenCounter
            .get()
    private val sessionActionCounter = AtomicLong()

    @Volatile private var conversation: Conversation? = null
    private var logicalKey: String? = null
    private var nextBefore: Long? = null

    @Volatile private var atNewest = false

    @Volatile private var foreground = false

    @Volatile private var newestLoaded = false
    private var listLimit = 50
    private var visibleStart = 0
    private var visibleCount = 10
    var featureAction: suspend (
        MessengerAction.Feature,
    ) -> Unit = {
    }
    var featureRefresh: suspend (
        ActiveSession,
        Conversation?,
    ) -> Unit = {
        _,
        _,
        ->
    }

    init {
        session.onMessage = {
            owner,
            message,
            ->
            projection.withLock {
                if (session
                        .accepts(
                            owner.key,
                        )
                ) {
                    if (message.conversationId == conversation?.id()) {
                        refreshTimeline(
                            owner,
                            screenGeneration,
                            preserve = true,
                        )
                    }
                    refreshList(owner)
                }
            }
        }
        session.onInvalidated = { owner ->
            projection.withLock {
                if (session
                        .accepts(
                            owner.key,
                        )
                ) {
                    refreshLoaded(owner)
                }
            }
        }
        viewModelScope.launch {
            session.active.collect { owner ->
                screenCounter
                    .incrementAndGet()
                conversation = null
                logicalKey = null
                cache
                    .clear()
                if (owner == null) {
                    ui.value =
                        MessengerState(
                            backend =
                                ui.value.backend,
                        )
                } else {
                    ui.value =
                        MessengerState(
                            screen =
                                Screen.CONVERSATIONS,
                            backend =
                                owner.profile.backend,
                            inbox =
                                owner.client
                                    .inboxId(),
                        )
                    projection.withLock {
                        refreshLoaded(owner)
                    }
                }
            }
        }
        viewModelScope.launch {
            session.error.collect { error ->
                if (error != null) {
                    ui.value =
                        ui.value
                            .copy(error = error)
                }
            }
        }
        viewModelScope.launch {
            session.readerError.collect {
                ui.value =
                    ui.value
                        .copy(readerError = it)
            }
        }
        viewModelScope.launch {
            session.connection.collect {
                ui.value =
                    ui.value
                        .copy(
                            connection =
                                if (it
                                        .contains(
                                            "Connected",
                                            true,
                                        )
                                ) {
                                    ""
                                } else {
                                    it
                                },
                        )
            }
        }
        viewModelScope.launch {
            try {
                session
                    .restore()
                if (session.active.value == null &&
                    session.preferences
                        .profiles()
                        .isEmpty()
                ) {
                    val legacy =
                        java.io
                            .File(
                                application.filesDir,
                                "xmtp_db",
                            )
                    val accounts =
                        legacy
                            .listFiles()
                            ?.filter {
                                it.name
                                    .matches(Regex("xmtp-local-[0-9a-f]{64}\\.db3"))
                            }?.map {
                                it.name
                                    .removePrefix("xmtp-local-")
                                    .removeSuffix(".db3")
                            }.orEmpty()
                    if (accounts
                            .isNotEmpty()
                    ) {
                        ui.value =
                            ui.value
                                .copy(
                                    migrationRequired = true,
                                    migrationAccounts = accounts,
                                )
                    }
                }
            } catch (error: Throwable) {
                showError(error)
            }
        }
    }

    fun setFeatures(value: FeatureAvailability) {
        ui.value =
            ui.value
                .copy(features = value)
    }

    fun foreground(value: Boolean) {
        foreground = value
        if (value) {
            dispatch(
                MessengerAction.Refresh,
            )
        }
    }

    internal var lookupConversation: suspend (ActiveSession, String) -> Conversation? = { owner, id ->
        owner.client.conversations
            .getById(
                id,
            )
    }
    internal var historyRead: suspend (
        Conversation,
        ListMessagesOptions,
    ) -> List<Message> = { chat, options -> chat.messages(options) }
    internal var historyCount: suspend (
        Conversation,
        ListMessagesOptions,
    ) -> ULong = { chat, options -> chat.countMessages(options) }

    fun dispatch(action: MessengerAction) {
        if (action is MessengerAction.Navigate) {
            navigate(
                action.screen,
            )
            return
        }
        if (action is MessengerAction.Reply) {
            ui.value =
                ui.value
                    .copy(
                        replyTo =
                            action.messageId,
                        replyPreview =
                            ui.value.messages
                                .firstOrNull {
                                    it.id ==
                                        action.messageId
                                }?.text,
                    )
            return
        }
        val requestedToken =
            if (action is MessengerAction.OpenConversation) screenCounter.incrementAndGet() else screenGeneration
        val actionScope =
            if (action is MessengerAction
                    .Connect ||
                action is MessengerAction
                    .ResetLegacyAccount || action ==
                MessengerAction
                    .SignOut || action ==
                MessengerAction.DeleteAccount
            ) {
                viewModelScope
            } else {
                session.active.value?.work ?: viewModelScope
            }
        val replacesSession =
            action is MessengerAction
                .Connect ||
                action is MessengerAction
                    .ResetLegacyAccount || action ==
                MessengerAction
                    .SignOut || action ==
                MessengerAction.DeleteAccount
        val operationToken =
            if (replacesSession) {
                sessionActionCounter
                    .incrementAndGet()
            } else {
                sessionActionCounter
                    .get()
            }
        actionScope
            .launch(
                Dispatchers.IO,
            ) {
                val owner =
                    session.active.value
                val token = requestedToken
                try {
                    when (action) {
                        is MessengerAction.Connect,
                        -> {
                            ui.value =
                                ui.value
                                    .copy(
                                        busy = true,
                                        error = null,
                                    )
                            session
                                .connect(
                                    action.backend,
                                    action.credential,
                                    action.allowPrivateNetwork,
                                )
                        }

                        MessengerAction.SignOut,
                        -> {
                            session
                                .signOut()
                        }

                        MessengerAction.DeleteAccount,
                        -> {
                            session
                                .deleteAccount()
                            ui.value =
                                MessengerState(
                                    backend =
                                        BuildConfig.XMTP_BACKEND_URL,
                                )
                        }

                        is MessengerAction.ResetLegacyAccount,
                        -> {
                            session
                                .resetLegacyAccount(
                                    action.inboxId,
                                )
                            ui.value =
                                ui.value
                                    .copy(
                                        migrationAccounts =
                                            ui.value
                                                .migrationAccounts -
                                                action.inboxId,
                                        migrationRequired =
                                            ui.value.migrationAccounts.size > 1,
                                    )
                        }

                        else -> {
                            if (owner == null ||
                                !session
                                    .accepts(
                                        owner.key,
                                    )
                            ) {
                                return@launch
                            }
                            when (action) {
                                is MessengerAction.OpenConversation,
                                -> {
                                    open(
                                        owner,
                                        action.id,
                                        token,
                                    )
                                }

                                is MessengerAction.SelectTab,
                                -> {
                                    projection.withLock {
                                        ui.value =
                                            ui.value
                                                .copy(
                                                    unknownTab =
                                                        action.unknown,
                                                )
                                        listLimit = 50
                                        visibleStart = 0
                                        refreshList(owner)
                                    }
                                }

                                is MessengerAction.ListViewport,
                                -> {
                                    projection.withLock {
                                        if (visibleStart !=
                                            action
                                                .firstIndex || visibleCount !=
                                            action.visibleCount
                                        ) {
                                            visibleStart =
                                                action.firstIndex
                                            visibleCount =
                                                action.visibleCount
                                            refreshList(owner)
                                        }
                                    }
                                }

                                MessengerAction.LoadMoreConversations,
                                -> {
                                    listLimit += 50
                                    projection.withLock {
                                        refreshList(owner)
                                    }
                                }

                                MessengerAction.Refresh,
                                -> {
                                    projection.withLock {
                                        refreshLoaded(owner)
                                    }
                                }

                                MessengerAction.RetryReader,
                                -> {
                                    session
                                        .retryReader()
                                }

                                MessengerAction.LoadOlder,
                                -> {
                                    projection.withLock {
                                        loadOlder(
                                            owner,
                                            token,
                                        )
                                    }
                                }

                                MessengerAction.JumpToLatest,
                                -> {
                                    projection.withLock {
                                        refreshTimeline(
                                            owner,
                                            token,
                                            preserve = false,
                                        )
                                    }
                                }

                                is MessengerAction.Viewport,
                                -> {
                                    saveViewport(
                                        owner,
                                        token,
                                        action,
                                    )
                                }

                                is MessengerAction.SendText,
                                -> {
                                    val chat = conversation ?: return@launch
                                    val reply =
                                        ui.value.replyTo
                                    sends
                                        .queue(
                                            owner.key,
                                            owner.client,
                                            chat,
                                            reconcile = {
                                                merge(
                                                    owner,
                                                    token,
                                                    it,
                                                )
                                            },
                                        ) {
                                            if (reply != null) {
                                                (
                                                    owner.client.conversations
                                                        .getMessageById(reply) ?: error("Reply parent unavailable")
                                                ).reply(
                                                    action.text,
                                                    SendOptions(optimistic = true),
                                                )
                                            } else {
                                                chat
                                                    .sendText(
                                                        action.text,
                                                        SendOptions(optimistic = true),
                                                    )
                                            }
                                        }
                                    if (valid(
                                            owner,
                                            token,
                                        )
                                    ) {
                                        ui.value =
                                            ui.value
                                                .copy(
                                                    replyTo = null,
                                                    replyPreview = null,
                                                )
                                    }
                                }

                                is MessengerAction.RetrySend,
                                -> {
                                    conversation?.let {
                                        sends
                                            .retry(
                                                owner.key,
                                                owner.client,
                                                it,
                                                action.messageId,
                                            ) { message ->
                                                merge(
                                                    owner,
                                                    token,
                                                    message,
                                                )
                                            }
                                    }
                                }

                                is MessengerAction.React,
                                -> {
                                    val message =
                                        owner.client.conversations
                                            .getMessageById(
                                                action.messageId,
                                            ) ?: error("Message unavailable")
                                    val chat = conversation ?: return@launch
                                    sends
                                        .queue(
                                            owner.key,
                                            owner.client,
                                            chat,
                                            reconcile = {
                                                merge(
                                                    owner,
                                                    token,
                                                    it,
                                                )
                                            },
                                        ) {
                                            message
                                                .react(
                                                    Reaction(
                                                        action.emoji,
                                                        if (action.remove) {
                                                            ReactionAction.REMOVED
                                                        } else {
                                                            ReactionAction.ADDED
                                                        },
                                                        ReactionSchema.UNICODE,
                                                    ),
                                                    SendOptions(optimistic = true),
                                                )
                                        }
                                    projection.withLock {
                                        refreshTimeline(
                                            owner,
                                            token,
                                            true,
                                        )
                                    }
                                }

                                is MessengerAction.DeleteMessage,
                                -> {
                                    conversation?.deleteMessage(
                                        action.messageId,
                                    )
                                    projection.withLock {
                                        refreshTimeline(
                                            owner,
                                            token,
                                            true,
                                        )
                                    }
                                }

                                is MessengerAction.Consent,
                                -> {
                                    conversation?.updateConsentState(
                                        if (action.allowed) {
                                            ConsentState.ALLOWED
                                        } else {
                                            ConsentState.DENIED
                                        },
                                    )
                                    if (!action.allowed) {
                                        navigate(
                                            Screen.CONVERSATIONS,
                                        )
                                    }
                                    projection.withLock {
                                        refreshLoaded(owner)
                                    }
                                }

                                is MessengerAction.Create,
                                -> {
                                    create(
                                        owner,
                                        action,
                                        token,
                                    )
                                }

                                is MessengerAction.DiscardUnknownSend,
                                -> {
                                    if (session
                                            .accepts(
                                                owner.key,
                                            )
                                    ) {
                                        session.preferences
                                            .removeDraft(
                                                owner.key.profileId,
                                                action.draftId,
                                                admit = { change -> session.admit(owner.key, change) },
                                            )
                                    }
                                    refreshUnknown(owner)
                                }

                                is MessengerAction.Feature,
                                -> {
                                    featureAction(action)
                                }

                                else -> {
                                    mutateGroup(
                                        owner,
                                        token,
                                        action,
                                    )
                                }
                            }
                        }
                    }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    if (operationToken ==
                        sessionActionCounter
                            .get() && (
                            owner == null ||
                                session
                                    .accepts(
                                        owner.key,
                                    )
                        )
                    ) {
                        showError(error)
                    }
                } finally {
                    if (operationToken ==
                        sessionActionCounter
                            .get() && (
                            owner == null ||
                                session
                                    .accepts(
                                        owner.key,
                                    )
                        )
                    ) {
                        ui.value =
                            ui.value
                                .copy(busy = false)
                    }
                }
            }
    }

    private fun navigate(screen: Screen) {
        screenCounter
            .incrementAndGet()
        atNewest = false
        ui.value =
            ui.value
                .copy(
                    screen = screen,
                    error = null,
                )
        if (screen == Screen.CONVERSATIONS || screen ==
            Screen.CONVERSATION_SETTINGS
        ) {
            dispatch(
                MessengerAction.Refresh,
            )
        }
    }

    private fun valid(
        owner: ActiveSession,
        token: Long,
    ) = session
        .accepts(
            owner.key,
        ) && token == screenGeneration

    private fun showError(error: Throwable) {
        if (error !is CancellationException) {
            ui.value =
                ui.value
                    .copy(
                        error =
                            error
                                .toString(),
                        busy = false,
                    )
        }
    }

    private suspend fun refreshUnknown(owner: ActiveSession) {
        val unknown =
            session.preferences
                .drafts(
                    owner.key.profileId,
                ).filter {
                    it.phase ==
                        SendPhase
                            .QUEUEING && it.acceptedMessageId == null &&
                        !sends
                            .isInFlight(
                                it.draftId,
                            )
                }.map {
                    UnknownSendRow(
                        it.draftId,
                        it.conversationKey,
                    )
                }
        if (session
                .accepts(
                    owner.key,
                )
        ) {
            ui.value =
                ui.value
                    .copy(unknownSends = unknown)
        }
    }

    private suspend fun refreshLoaded(owner: ActiveSession) {
        refreshList(owner)
        refreshUnknown(owner)
        if (conversation != null) {
            refreshTimeline(
                owner,
                screenGeneration,
                true,
            )
            if (ui.value.screen ==
                Screen.CONVERSATION_SETTINGS
            ) {
                refreshSettings(
                    owner,
                    screenGeneration,
                )
            }
        }
        featureRefresh(
            owner,
            conversation,
        )
    }

    private suspend fun refreshList(owner: ActiveSession) =
        coroutineScope {
            val unknown =
                ui.value.unknownTab
            val chats =
                owner.client.conversations
                    .list(
                        ListConversationsOptions(
                            limit =
                                listLimit
                                    .toUInt(),
                            consentStates =
                                listOf(
                                    if (unknown) {
                                        ConsentState.UNKNOWN
                                    } else {
                                        ConsentState.ALLOWED
                                    },
                                ),
                        ),
                    )
            val previous =
                ui.value.conversations.associateBy {
                    it.id
                }
            val rows =
                chats
                    .mapIndexed {
                        index,
                        chat,
                        ->
                        async {
                            if (index < visibleStart || index >= visibleStart + visibleCount + 50) {
                                return@async previous[
                                    chat
                                        .id(),
                                ] ?: ConversationRow(
                                    chat
                                        .id(),
                                    chat
                                        .id()
                                        .take(12),
                                    "",
                                    "",
                                    "0",
                                    unknown,
                                    chat
                                        .id()
                                        .hashCode(),
                                )
                            }
                            reads.withPermit {
                                val state = conversationState(chat)
                                val title =
                                    when (chat) {
                                        is Conversation.Group,
                                        -> {
                                            chat.group
                                                .state()
                                                .name
                                                .ifBlank {
                                                    "Group"
                                                }
                                        }

                                        is Conversation.Dm,
                                        -> {
                                            chat.dm
                                                .peerInboxId()
                                                ?.take(12) ?: "Direct message"
                                        }
                                    }
                                val key =
                                    logicalConversationKey(
                                        chat,
                                        owner.client
                                            .inboxId(),
                                    )
                                val marker =
                                    session.preferences
                                        .marker(
                                            owner.key.profileId,
                                            key,
                                        )
                                val unread =
                                    chat
                                        .countMessages(
                                            incomingSelection(
                                                owner.client
                                                    .inboxId(),
                                                marker.insertedAtNs,
                                            ),
                                        )
                                val last =
                                    chat
                                        .messages(
                                            publishedSelection()
                                                .copy(limit = 1u),
                                        ).firstOrNull()
                                        ?.toRow(
                                            owner.client
                                                .inboxId(),
                                        )
                                ConversationRow(
                                    chat
                                        .id(),
                                    title,
                                    last
                                        ?.text
                                        .orEmpty(),
                                    last
                                        ?.time
                                        .orEmpty(),
                                    unread
                                        .toString(),
                                    state.consentState ==
                                        ConsentState.UNKNOWN,
                                    chat
                                        .id()
                                        .hashCode(),
                                )
                            }
                        }
                    }.awaitAll()
            if (session
                    .accepts(
                        owner.key,
                    ) && ui.value.unknownTab == unknown
            ) {
                ui.value =
                    ui.value
                        .copy(conversations = rows)
            }
        }

    internal var onOpenFinished: (String) -> Unit = {}

    private suspend fun open(
        owner: ActiveSession,
        id: String,
        token: Long,
    ) {
        try {
            performOpen(owner, id, token)
        } finally {
            onOpenFinished(id)
        }
    }

    private suspend fun performOpen(
        owner: ActiveSession,
        id: String,
        token: Long,
    ) {
        if (!valid(owner, token)) return
        val chat = lookupConversation(owner, id) ?: error("Conversation unavailable")
        if (!valid(owner, token)) return
        val state = conversationState(chat)
        if (!valid(owner, token)) return
        check(state.consentState != ConsentState.DENIED) { "This conversation is blocked" }
        val key = logicalConversationKey(chat, owner.client.inboxId())
        if (!valid(owner, token)) return
        val anchor = session.preferences.anchor(owner.key.profileId, key)
        if (!valid(owner, token)) return
        conversation = chat
        logicalKey = key
        atNewest = false
        newestLoaded = false
        nextBefore = null
        ui.value =
            ui.value.copy(
                screen = Screen.TIMELINE,
                conversationId = id,
                conversationTitle =
                    ui.value.conversations
                        .firstOrNull { it.id == id }
                        ?.title ?: id.take(12),
                conversationUnknown = state.consentState == ConsentState.UNKNOWN,
                messages = emptyList(),
                anchor = null,
                replyTo = null,
                replyPreview = null,
                error = null,
                hasOlder = true,
            )
        projection.withLock {
            if (!valid(owner, token)) return
            val cached = cache.get(id)
            if (cached != null && anchor != null && cached.any { it.id == anchor.messageId }) {
                ui.value = ui.value.copy(messages = cached.map { it.toRow(owner.client.inboxId()) }, anchor = anchor)
                nextBefore = cached.lastOrNull()?.sentAt?.ns
                refreshTimeline(owner, token, true)
            } else if (anchor != null && !anchor.wasAtNewest) {
                restorePosition(owner, token, anchor)
            } else {
                refreshTimeline(owner, token, false)
            }
        }
    }

    private suspend fun page(
        chat: Conversation,
        before: Long?,
    ) = TimestampBuckets<Message>({ it.sentAt.ns }).load(
        before,
        read = { upper, limit ->
            historyRead(
                chat,
                publishedSelection().copy(sentBefore = upper?.let(::Timestamp), limit = limit.toUInt()),
            )
        },
        count = { upper, lower ->
            historyCount(
                chat,
                publishedSelection().copy(sentBefore = upper?.let(::Timestamp), sentAfter = lower?.let(::Timestamp)),
            )
        },
    )

    private suspend fun overlay(chat: Conversation) =
        chat
            .messages(
                publishedSelection()
                    .copy(
                        deliveryStatus =
                            DeliveryStatus.UNPUBLISHED,
                        limit = 25u,
                    ),
            ) +
            chat
                .messages(
                    publishedSelection()
                        .copy(
                            deliveryStatus =
                                DeliveryStatus.FAILED,
                            limit = 25u,
                        ),
                )

    private suspend fun refreshTimeline(
        owner: ActiveSession,
        token: Long,
        preserve: Boolean,
    ) {
        val chat = conversation ?: return
        val previous =
            if (preserve) {
                cache
                    .get(
                        chat
                            .id(),
                    ).orEmpty()
            } else {
                emptyList()
            }
        val oldest =
            previous.minOfOrNull {
                it.sentAt.ns
            }
        var result =
            page(
                chat,
                null,
            )
        val rows =
            result.rows
                .toMutableList()
        while (result.notice == null &&
            !result
                .complete && rows.size < 500 && oldest != null && (
                result.nextBeforeNs ?: Long.MIN_VALUE
            ) > oldest
        ) {
            result =
                page(
                    chat,
                    result.nextBeforeNs,
                )
            rows
                .addAll(
                    result.rows,
                )
        }
        val queued = overlay(chat)
        if (!valid(
                owner,
                token,
            ) || chat
                .id() != conversation?.id()
        ) {
            return
        }
        nextBefore =
            result.nextBeforeNs
        val retained =
            cache
                .put(
                    chat
                        .id(),
                    rows,
                    ui.value.anchor?.messageId,
                )
        newestLoaded = rows
            .isEmpty() ||
            retained.any {
                it.id ==
                    rows
                        .maxBy { row ->
                            row.sentAt.ns
                        }.id
            }
        val previousAnchor =
            ui.value.anchor.takeIf {
                preserve
            }
        val position =
            previousAnchor?.let {
                restoreAnchor(
                    it,
                    retained.map { row ->
                        row
                            .toRow(
                                owner.client
                                    .inboxId(),
                            )
                    },
                )
            }
        val positionLost = position?.changed == true
        val restoredAnchor =
            if (!preserve) {
                retained
                    .firstOrNull()
                    ?.let {
                        ScrollAnchor(
                            it.id,
                            it.sentAt.ns,
                            0,
                            true,
                        )
                    }
            } else {
                position?.anchor
            }
        val consent =
            conversationState(chat).consentState
        if (!valid(
                owner,
                token,
            )
        ) {
            return
        }
        ui.value =
            ui.value
                .copy(
                    messages =
                        (retained + queued)
                            .associateBy {
                                it.id
                            }.values
                            .sortedWith(
                                compareByDescending<Message> {
                                    it.sentAt.ns
                                }.thenBy {
                                    it.id
                                },
                            ).map {
                                it
                                    .toRow(
                                        owner.client
                                            .inboxId(),
                                    )
                            },
                    historyNotice =
                        if (positionLost) {
                            "Position changed"
                        } else {
                            result.notice
                        },
                    hasOlder =
                        !result
                            .complete && result.notice == null,
                    conversationUnknown =
                        consent ==
                            ConsentState.UNKNOWN,
                    anchor = restoredAnchor,
                )
        markRead(
            owner,
            token,
        )
    }

    private suspend fun loadOlder(
        owner: ActiveSession,
        token: Long,
    ) {
        val chat = conversation ?: return
        val result =
            page(
                chat,
                nextBefore,
            )
        val queued = overlay(chat)
        if (!valid(
                owner,
                token,
            )
        ) {
            return
        }
        nextBefore =
            result.nextBeforeNs
        val before =
            cache
                .get(
                    chat
                        .id(),
                ).orEmpty()
        val newestId =
            before
                .firstOrNull()
                ?.id
        val rows =
            cache
                .put(
                    chat
                        .id(),
                    before +
                        result.rows,
                    ui.value.anchor?.messageId,
                )
        newestLoaded = newestLoaded && (
            newestId == null ||
                rows.any {
                    it.id == newestId
                }
        )
        ui.value =
            ui.value
                .copy(
                    messages =
                        (rows + queued)
                            .associateBy {
                                it.id
                            }.values
                            .sortedByDescending {
                                it.sentAt.ns
                            }.map {
                                it
                                    .toRow(
                                        owner.client
                                            .inboxId(),
                                    )
                            },
                    historyNotice =
                        result.notice,
                    hasOlder =
                        !result
                            .complete && result.notice == null,
                )
    }

    private suspend fun restorePosition(
        owner: ActiveSession,
        token: Long,
        saved: ScrollAnchor,
    ) {
        val chat = conversation ?: return
        val before = if (saved.sentAtNs == Long.MAX_VALUE) null else saved.sentAtNs + 1
        val result = page(chat, before)
        if (!valid(owner, token)) return
        nextBefore = result.nextBeforeNs
        if (result.rows.isEmpty() && result.notice == null) {
            refreshTimeline(owner, token, false)
            if (valid(owner, token)) ui.value = ui.value.copy(historyNotice = "Position changed")
            return
        }
        val retained = cache.put(chat.id(), result.rows, saved.messageId)
        val position = restoreAnchor(saved, retained.map { it.toRow(owner.client.inboxId()) })
        newestLoaded = false
        ui.value =
            ui.value.copy(
                messages = retained.map { it.toRow(owner.client.inboxId()) },
                anchor = position.anchor,
                historyNotice = result.notice ?: if (position.changed) "Position changed" else null,
                hasOlder = !result.complete && result.notice == null,
            )
    }

    private suspend fun merge(
        owner: ActiveSession,
        token: Long,
        message: Message,
    ) {
        if (valid(
                owner,
                token,
            ) && message.conversationId == conversation?.id()
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

    private suspend fun saveViewport(
        owner: ActiveSession,
        token: Long,
        action: MessengerAction.Viewport,
    ) = projection.withLock {
        if (!valid(
                owner,
                token,
            ) || ui.value.screen !=
            Screen.TIMELINE
        ) {
            return
        }
        atNewest =
            action.atNewest
        val key = logicalKey ?: return
        session.preferences
            .saveAnchor(
                owner.key.profileId,
                key,
                action.anchor,
            )
        if (valid(
                owner,
                token,
            )
        ) {
            ui.value =
                ui.value
                    .copy(
                        anchor =
                            action.anchor,
                    )
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
                    ) && ui.value.screen ==
                    Screen.TIMELINE
            },
            latestIncoming = {
                chat
                    .messages(
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
                    )
                refreshUnread(owner, chat, key)
            },
        )
    }

    private suspend fun refreshUnread(
        owner: ActiveSession,
        chat: Conversation,
        key: String,
    ) {
        val marker = session.preferences.marker(owner.key.profileId, key).insertedAtNs
        val count = chat.countMessages(incomingSelection(owner.client.inboxId(), marker)).toString()
        session.withCurrent(owner.key) {
            ui.value =
                ui.value.copy(
                    conversations =
                        ui.value.conversations.map {
                            if (it.id == chat.id()) it.copy(unread = count) else it
                        },
                )
        }
    }

    private suspend fun create(
        owner: ActiveSession,
        action: MessengerAction.Create,
        token: Long,
    ) {
        val values =
            action.recipients
                .split(',')
                .map(String::trim)
                .filter(String::isNotEmpty)
        require(
            values
                .isNotEmpty(),
        )
        val inboxes =
            values.map { value ->
                if (value
                        .startsWith("0x")
                ) {
                    owner.client
                        .inboxIdFor(
                            PublicIdentity(
                                value,
                                PublicIdentityKind.ETHEREUM,
                            ),
                        ) ?: error("Identity is not registered")
                } else {
                    require(
                        value
                            .matches(Regex("[0-9a-f]{64}")),
                    ) {
                        "Use lowercase inbox IDs"
                    }
                    value
                }
            }
        require(
            !action
                .group || (
                inboxes
                    .distinct()
                    .size + 1
            ).toULong() <=
                owner.client
                    .serverConfiguration()
                    .mls.maxGroupMembers,
        ) {
            "Too many members for this backend"
        }
        val chat =
            if (action.group) {
                Conversation
                    .Group(
                        owner.client.conversations
                            .createGroup(
                                inboxes,
                                CreateGroupOptions(
                                    permissions =
                                        if (action.adminOnly) {
                                            GroupPermissionMode.AdminOnly
                                        } else {
                                            GroupPermissionMode.AllMembers
                                        },
                                    name =
                                        action.name,
                                    description =
                                        action.description,
                                ),
                            ),
                    )
            } else {
                require(
                    inboxes.size == 1,
                ) {
                    "A direct message needs one recipient"
                }
                Conversation
                    .Dm(
                        owner.client.conversations
                            .createDm(
                                inboxes
                                    .single(),
                            ),
                    )
            }
        if (session
                .accepts(
                    owner.key,
                )
        ) {
            open(
                owner,
                chat
                    .id(),
                token,
            )
        }
    }

    private suspend fun mutateGroup(
        owner: ActiveSession,
        token: Long,
        action: MessengerAction,
    ) {
        val chat = conversation ?: return
        try {
            if (action is MessengerAction.SetDisappearing) {
                require(
                    action.seconds >= 0,
                )
                chat
                    .updateDisappearingSettings(
                        if (action.seconds == 0L) {
                            null
                        } else {
                            DisappearingSettings(
                                Timestamp(
                                    System
                                        .currentTimeMillis() * 1_000_000,
                                ),
                                Math
                                    .multiplyExact(
                                        action.seconds,
                                        1_000_000_000,
                                    ),
                            )
                        },
                    )
            } else {
                val group =
                    (
                        chat as? Conversation.Group
                    )?.group ?: error("This action needs a group")
                when (action) {
                    is MessengerAction.UpdateGroup,
                    -> {
                        group
                            .updateName(
                                action.name,
                            )
                        group
                            .updateDescription(
                                action.description,
                            )
                    }

                    is MessengerAction.AddMember,
                    -> {
                        group
                            .addMembers(
                                listOf(
                                    action.inboxId,
                                ),
                            )
                    }

                    is MessengerAction.RemoveMember,
                    -> {
                        group
                            .removeMembers(
                                listOf(
                                    action.inboxId,
                                ),
                            )
                    }

                    is MessengerAction.SetAdmin,
                    -> {
                        if (action.admin) {
                            group
                                .addAdmin(
                                    action.inboxId,
                                )
                        } else {
                            group
                                .removeAdmin(
                                    action.inboxId,
                                )
                        }
                    }

                    MessengerAction.RequestRemoval,
                    -> {
                        group
                            .requestRemoval()
                    }

                    is MessengerAction.SetPreset,
                    -> {
                        applyStandardPreset(
                            group,
                            action.adminOnly,
                        )
                    }

                    else -> {
                        Unit
                    }
                }
            }
        } finally {
            if (valid(
                    owner,
                    token,
                )
            ) {
                refreshSettings(
                    owner,
                    token,
                )
            }
        }
    }

    private suspend fun refreshSettings(
        owner: ActiveSession,
        token: Long,
    ) {
        val chat = conversation ?: return
        val own =
            owner.client
                .inboxId()
        val members =
            chat
                .members()
        val group =
            (
                chat as? Conversation.Group
            )?.group?.state()
        val common =
            group?.common ?: (
                chat as Conversation.Dm
            ).dm
                .state()
        val canManage = group?.admins?.contains(own) == true || group?.superAdmins?.contains(own) == true
        val settings =
            ConversationSettings(
                group?.name ?: ui.value.conversationTitle,
                group
                    ?.description
                    .orEmpty(),
                group != null,
                members.map {
                    MemberRow(
                        it.inboxId,
                        when (
                            it.permissionLevel
                        ) {
                            PermissionLevel.ADMIN,
                            -> "Admin"

                            PermissionLevel.SUPER_ADMIN,
                            -> "Super admin"

                            else -> "Member"
                        },
                        canManage && it.inboxId != own && it.permissionLevel !=
                            PermissionLevel.SUPER_ADMIN,
                    )
                },
                policyLabel(group?.permissions?.policyType),
                membershipLabel(group?.membershipState),
                group != null && members.size > 1 &&
                    group.superAdmins.none {
                        it == own
                    } && group.membershipState !=
                    MembershipState.PENDING_REMOVE,
                (
                    (
                        common.disappearingSettings?.retentionNs ?: 0L
                    ) / 1_000_000_000
                ).toString(),
                common.notificationsEnabled,
            )
        if (valid(
                owner,
                token,
            )
        ) {
            ui.value =
                ui.value
                    .copy(settings = settings)
        }
    }

    fun currentConversation(): Conversation? = conversation

    fun screenToken() = screenGeneration

    fun acceptsScreen(
        key: SessionKey,
        token: Long,
    ) = session
        .accepts(key) && token == screenGeneration

    suspend fun reconcileFeatureMessage(
        owner: ActiveSession,
        token: Long,
        message: Message,
    ) = merge(
        owner,
        token,
        message,
    )
}
