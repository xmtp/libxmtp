package org.xmtp.android.example.messenger.feature.conversations

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.sync.withPermit
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.messenger.app.DestinationScope
import org.xmtp.android.example.messenger.data.xmtp.ChatQueries
import org.xmtp.android.example.messenger.data.xmtp.ConversationQueries
import org.xmtp.android.example.messenger.feature.AppFeaturePreferences
import org.xmtp.android.example.messenger.feature.FeatureInputFailure
import org.xmtp.android.example.messenger.feature.featureFailure
import org.xmtp.android.example.messenger.notifications.NotificationController
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.conversations.*
import uniffi.xmtp_sdk.*
import java.util.concurrent.atomic.AtomicLong

/** The SDK rejects a conversation list limit above this. */
private const val MAX_LIST_LIMIT = 1000

internal class ConversationsController(
    private val session: AppSession,
    private val queries: ConversationQueries,
    private val chats: ChatQueries,
    private val preferences: AppFeaturePreferences,
    private val notifications: NotificationController,
    private val reads: Semaphore,
    private val destinations: DestinationScope,
    private val scope: CoroutineScope,
) {
    private val ui = MutableStateFlow(ConversationsState())
    val state: StateFlow<ConversationsState> = ui
    private val requestUi = MutableStateFlow(RequestState(deleteCapability = CapabilityUi(true)))
    val requestState: StateFlow<RequestState> = requestUi

    private data class RequestCommand(
        val token: Long,
        val decision: RequestDecision,
    )

    private var nextRequestCommand = 0L
    private val requestCommands = mutableMapOf<Pair<SessionKey, String>, RequestCommand>()
    private val projection = Mutex()
    private val generation = AtomicLong()
    private var visibleStart = 0
    private var visibleCount = 10
    private var knownRows = emptyList<ConversationRow>()
    private var requestRows = emptyList<ConversationRow>()
    internal var listGroupStateRead: suspend (Group) -> uniffi.xmtp_sdk.GroupState = queries::groupState
    internal var writeConsent: suspend (
        Conversation,
        ConsentState,
    ) -> Unit =
        {
            chat,
            consent,
            ->
            chat.updateConsentState(consent)
        }
    internal var onConsentFinished: () -> Unit = {}
    internal var beforeConsentDecision: suspend (RequestDecision) -> Unit = {}
    internal var historyPageRead: suspend (
        Conversation,
        ListMessagesOptions,
        MessageHistoryPosition?,
        MessageHistoryPosition?,
    ) -> MessageHistoryPage =
        chats::history

    fun clear() {
        session.withStateLock { requestCommands.clear() }
        generation.incrementAndGet()
        knownRows =
            emptyList()
        requestRows =
            emptyList()
        ui.value =
            ConversationsState()
        requestUi.value =
            RequestState(
                deleteCapability =
                    CapabilityUi(true),
            )
    }

    fun showRequests(value: Boolean) {
        if (ui.value.unknownTab != value) {
            generation.incrementAndGet()
            ui.update {
                it.copy(
                    unknownTab =
                    value,
                    listLimit =
                    50,
                    anchor =
                        ListAnchor(),
                )
            }
            visibleStart =
                0
            publishRows()
        }
    }

    fun row(id: String): ConversationRow? =
        knownRows.firstOrNull {
            it.id == id
        } ?: requestRows.firstOrNull {
            it.id == id
        }

    fun dispatch(action: ConversationsAction) {
        when (action) {
            is ConversationsAction.ChangeQuery -> {
                ui.update { it.copy(query = action.value) }
                publishRows()
            }

            is ConversationsAction.OpenConversation -> {
                ui.update {
                    it.copy(
                        selectedConversationId =
                            action.id,
                    )
                }
                destinations.navigate(AppRoute.Chat(action.id))
            }

            ConversationsAction.OpenRequests -> {
                ui.update {
                    it.copy(
                        unknownTab =
                        true,
                    )
                }
                destinations.navigate(AppRoute.Requests)
                refresh()
            }

            is ConversationsAction.Create -> {
                destinations.navigate(AppRoute.CreateConversation)
            }

            is ConversationsAction.SelectTab -> {
                showRequests(action.unknown)
                destinations.navigate(if (action.unknown) AppRoute.Requests else AppRoute.Chats)
                refresh()
            }

            is ConversationsAction.Viewport -> {
                ui.update { it.copy(anchor = ListAnchor(action.firstIndex, action.offsetPx)) }
                if (ui.value.query.isNotBlank()) return
                if (visibleStart != action.firstConversationIndex ||
                    visibleCount != action.visibleCount
                ) {
                    visibleStart =
                        action.firstConversationIndex
                    visibleCount =
                        action.visibleCount
                    refresh()
                }
            }

            ConversationsAction.LoadMore -> {
                if (ui.value.pageLoading || !ui.value.hasMore) return
                ui.update { it.copy(pageLoading = true, listLimit = minOf(it.listLimit + 50, MAX_LIST_LIMIT)) }
                refresh()
            }

            ConversationsAction.Refresh -> {
                refresh()
            }

            is ConversationsAction.SetAlias -> {
                val owner = session.active.value ?: return
                owner.work.launch {
                    preferences.setAlias(
                        owner.key,
                        action.conversationId,
                        action.alias,
                    )
                    refresh(owner)
                }
            }
        }
    }

    private fun publishRows() {
        val query = ui.value.query.trim()

        fun filter(rows: List<ConversationRow>) =
            if (query.isEmpty()) {
                rows
            } else {
                rows.filter {
                    it.title.contains(
                        query,
                        true,
                    ) ||
                        it.preview.contains(
                            query,
                            true,
                        ) ||
                        it.id.contains(
                            query,
                            true,
                        )
                }
            }
        ui.update {
            it.copy(
                rows =
                    filter(if (it.unknownTab) requestRows else knownRows),
                requests =
                    filter(requestRows),
            )
        }
    }

    fun refresh() {
        val owner =
            session.active.value ?: return
        owner.work.launch {
            try {
                refresh(owner)
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
            }
        }
    }

    suspend fun refresh(owner: ActiveSession): List<ConversationRow> =
        projection.withLock {
            val version = generation.get()
            val selected = ui.value.unknownTab
            val limit = ui.value.listLimit
            ui.update { it.copy(busy = true, failure = null) }
            try {
                coroutineScope {
                    val known =
                        queries.list(
                            owner,
                            ListConversationsOptions(
                                limit =
                                    minOf(limit + 1, MAX_LIST_LIMIT).toUInt(),
                                consentStates =
                                    listOf(ConsentState.ALLOWED),
                            ),
                        )
                    val allRequests =
                        queries.list(
                            owner,
                            ListConversationsOptions(
                                limit =
                                501u,
                                consentStates =
                                    listOf(ConsentState.UNKNOWN),
                            ),
                        )
                    val requests = allRequests.filterNot { preferences.dismissed(owner.key.profileId, it.id()) }
                    val own = owner.client.inboxId()

                    suspend fun rows(
                        chats: List<Conversation>,
                        unknown: Boolean,
                        previous: List<ConversationRow>,
                    ): List<ConversationRow> {
                        val old = previous.associateBy { it.id }
                        return chats
                            .take(limit)
                            .mapIndexed { index, chat ->
                                async {
                                    if (index < visibleStart ||
                                        index >= visibleStart + visibleCount + 50
                                    ) {
                                        return@async old[chat.id()] ?: ConversationRow(
                                            chat.id(),
                                            chat.id().take(12),
                                            "",
                                            "",
                                            "0",
                                            unknown,
                                            chat.id().hashCode(),
                                        )
                                    }
                                    reads.withPermit {
                                        val group = (chat as? Conversation.Group)?.let { listGroupStateRead(it.group) }
                                        val common = group?.common ?: queries.state(chat)
                                        val alias = preferences.alias(owner.key.profileId, chat.id())
                                        val title = alias ?: chat.displayTitle(groupName = group?.name)
                                        val logical = logicalConversationKey(chat, own)
                                        val marker = session.preferences.marker(owner.key.profileId, logical)
                                        val unread = queries.count(chat, incomingSelection(own, marker.insertedAtNs))
                                        val last =
                                            historyPageRead(
                                                chat,
                                                publishedSelection().copy(
                                                    limit =
                                                    1u,
                                                ),
                                                null,
                                                null,
                                            ).messages.firstOrNull()?.toRow(own)
                                        ConversationRow(
                                            chat.id(),
                                            title,
                                            last?.text.orEmpty(),
                                            last?.time.orEmpty(),
                                            unread.toString(),
                                            common.consentState == ConsentState.UNKNOWN,
                                            chat.id().hashCode(),
                                            muted =
                                                notifications.muted(owner, logical),
                                            group =
                                                group != null,
                                            alias =
                                            alias,
                                            senderPreview =
                                                last?.sender,
                                        )
                                    }
                                }
                            }.awaitAll()
                    }
                    val nextKnown = rows(known, false, knownRows)
                    val nextRequests = rows(requests, true, requestRows)
                    if (session.accepts(owner.key) && version == generation.get()) {
                        knownRows = nextKnown
                        requestRows = nextRequests
                        ui.update {
                            it.copy(
                                requestCount =
                                    requests.size,
                                requestCountValid =
                                    allRequests.size < 501,
                                hasMore =
                                    if (selected) requests.size > limit else known.size > limit,
                            )
                        }
                        publishRows()
                    }
                }
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                if (session.accepts(owner.key) &&
                    version == generation.get()
                ) {
                    ui.update {
                        it.copy(
                            failure =
                                featureFailure(
                                    error,
                                    "Cannot update chats. Try again.",
                                    "conversation-list",
                                ),
                        )
                    }
                }
                throw error
            } finally {
                if (session.accepts(owner.key) &&
                    version == generation.get()
                ) {
                    ui.update {
                        it.copy(
                            busy =
                            false,
                            pageLoading =
                            false,
                        )
                    }
                }
            }
            ui.value.rows
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

    fun prepareRequest(
        owner: ActiveSession,
        id: String,
        title: String,
    ) {
        val origin = destinations.capture(owner)
        destinations.admit(origin) {
            if (routeConversationId() == id) {
                val active = requestCommands[owner.key to id]?.decision
                if (requestUi.value.conversationId != id) {
                    requestUi.value =
                        RequestState(
                            id,
                            title,
                            deleteCapability = CapabilityUi(true, "Remove this request from this device."),
                            activeDecision = active,
                        )
                } else {
                    requestUi.update { it.copy(activeDecision = active) }
                }
            }
        }
    }

    suspend fun bindRequest(
        owner: ActiveSession,
        chat: Conversation,
        title: String,
    ) {
        val origin = destinations.capture(owner)
        prepareRequest(owner, chat.id(), title)
        val sender = (chat as? Conversation.Dm)?.dm?.peerInboxId().orEmpty()
        destinations.admit(origin) {
            if (routeConversationId() == chat.id() &&
                requestUi.value.conversationId == chat.id()
            ) {
                requestUi.update {
                    it.copy(
                        title =
                        title,
                        senderInboxId =
                        sender,
                    )
                }
            }
        }
    }

    fun dispatch(action: RequestAction) {
        session.withStateLock {
            val current = requestUi.value
            if (current.activeDecision != null) return@withStateLock
            if (current.pendingDecision != null && action != RequestAction.Cancel && action != RequestAction.Confirm) {
                return@withStateLock
            }
            when (action) {
                RequestAction.Cancel -> {
                    requestUi.update { it.copy(pendingDecision = null) }
                }

                RequestAction.Accept -> {
                    decide(RequestDecision.ACCEPT)
                }

                RequestAction.DeleteLocal -> {
                    if (current.deleteCapability.available) {
                        requestUi.update { it.copy(pendingDecision = RequestDecision.DELETE_LOCAL) }
                    }
                }

                RequestAction.Block -> {
                    requestUi.update { it.copy(pendingDecision = RequestDecision.BLOCK) }
                }

                RequestAction.Confirm -> {
                    current.pendingDecision?.let(::decide)
                }
            }
        }
    }

    fun consent(allowed: Boolean) {
        decide(if (allowed) RequestDecision.ACCEPT else RequestDecision.BLOCK)
    }

    private fun decide(decision: RequestDecision) {
        val admitted =
            session.withStateLock {
                val owner = session.active.value ?: return@withStateLock null
                val current = requestUi.value
                val id = current.conversationId.takeIf(String::isNotBlank) ?: return@withStateLock null
                val key = owner.key to id
                if (requestCommands.containsKey(key) || routeConversationId() != id) return@withStateLock null
                if (current.pendingDecision != null && current.pendingDecision != decision) return@withStateLock null
                val origin = destinations.capture(owner)
                var command: RequestCommand? = null
                destinations.admit(origin) {
                    command = RequestCommand(++nextRequestCommand, decision)
                    requestCommands[key] = checkNotNull(command)
                    requestUi.update { it.copy(pendingDecision = null, activeDecision = decision, failure = null) }
                }
                command?.let { Triple(owner, origin, id to it) }
            } ?: return
        val (owner, origin, entry) = admitted
        val (id, command) = entry
        val key = owner.key to id
        val job =
            owner.work.launch {
                try {
                    beforeConsentDecision(decision)
                    val chat =
                        queries.find(owner, id) ?: throw FeatureInputFailure("This request is no longer available.")
                    if (!destinations.accepts(origin) ||
                        requestUi.value.conversationId != id ||
                        routeConversationId() != id
                    ) {
                        throw CancellationException("Request changed")
                    }
                    if (decision == RequestDecision.DELETE_LOCAL) {
                        preferences.dismiss(owner.key, id)
                    } else {
                        notifications.withPrivacyMutation(owner) {
                            if (!destinations.accepts(origin) ||
                                routeConversationId() != id
                            ) {
                                throw CancellationException("Request changed")
                            }
                            writeConsent(
                                chat,
                                if (decision == RequestDecision.ACCEPT) ConsentState.ALLOWED else ConsentState.DENIED,
                            )
                        }
                    }
                    if (destinations.accepts(origin)) {
                        if (decision != RequestDecision.ACCEPT) destinations.navigate(AppRoute.Chats)
                        refresh(owner)
                    }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    session.withStateLock {
                        if (session.accepts(owner.key) && requestUi.value.conversationId == id &&
                            requestCommands[key]?.token == command.token
                        ) {
                            requestUi.update {
                                it.copy(
                                    failure =
                                        featureFailure(
                                            error,
                                            "Cannot change this request. Try again.",
                                            "request-decision",
                                        ),
                                )
                            }
                        }
                    }
                }
            }
        job.invokeOnCompletion {
            session.withStateLock {
                if (requestCommands[key]?.token == command.token) {
                    requestCommands.remove(key)
                    if (session.accepts(owner.key) && requestUi.value.conversationId == id) {
                        requestUi.update { it.copy(activeDecision = null) }
                    }
                }
            }
            onConsentFinished()
        }
    }

    suspend fun refreshUnread(
        owner: ActiveSession,
        chat: Conversation,
        logical: String,
    ) = projection.withLock {
        val count =
            queries
                .count(
                    chat,
                    incomingSelection(
                        owner.client.inboxId(),
                        session.preferences
                            .marker(
                                owner.key.profileId,
                                logical,
                            ).insertedAtNs,
                    ),
                ).toString()
        if (session.accepts(owner.key)) {
            knownRows =
                knownRows.map {
                    if (it.id == chat.id()) {
                        it.copy(
                            unread =
                            count,
                        )
                    } else {
                        it
                    }
                }
            requestRows =
                requestRows.map {
                    if (it.id == chat.id()) {
                        it.copy(
                            unread =
                            count,
                        )
                    } else {
                        it
                    }
                }
            publishRows()
        }
    }

    internal suspend fun performanceList(owner: ActiveSession) = refresh(owner)
}
