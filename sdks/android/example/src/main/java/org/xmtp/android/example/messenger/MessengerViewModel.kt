package org.xmtp.android.example.messenger
import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.sync.withPermit
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.ExampleApp
import org.xmtp.android.example.messenger.metadata.MetadataEditorController
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.metadata.*
import uniffi.xmtp_sdk.*
import java.util.UUID
import java.util.concurrent.atomic.AtomicLong

class MessengerViewModel(
    application: Application,
) : AndroidViewModel(application) {
    val session =
        (application as ExampleApp).session
    val notifications = (application as ExampleApp).notifications
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
    private val metadataUi = MutableStateFlow(MetadataEditorState())
    val metadataState: StateFlow<MetadataEditorState> = metadataUi
    private val metadataMutex = Mutex()

    private data class MetadataBinding(
        val key: SessionKey,
        val token: Long,
        val conversationId: String,
        val controller: MetadataEditorController,
    )

    @Volatile private var metadataBinding: MetadataBinding? = null
    private var metadataJob: Job? = null
    private val reads = Semaphore(4)
    private val projection = Mutex()
    private val cache =
        SDKTranscriptCache<Message>({ it.id }, { it.historyPosition() })

    @Volatile private var selectedBackend = BuildConfig.XMTP_BACKEND_URL.trim().trimEnd('/')
    private val screenLock = Any()
    private val screenCounter = AtomicLong()
    private val screenGeneration get() =
        screenCounter
            .get()
    private val sessionActionCounter = AtomicLong()
    private var startupRestoreJob: Job? = null

    @Volatile private var conversation: Conversation? = null
    private var logicalKey: String? = null
    private var nextBefore: MessageHistoryPosition? = null
    private var recoveryUpper: MessageRecoveryPosition? = null
    private var recoveryNext: MessageRecoveryPosition? = null

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
        featureRefresh = { owner, _ -> owner.work.async { notifications.refreshStatus(owner) }.await() }
        featureAction = { action ->
            val owner = session.active.value
            when (action.name) {
                "app-notifications" -> {
                    owner?.work?.async { notifications.setEnabled(owner, action.value == "true") }?.await()
                }

                "conversation-notifications" -> {
                    owner
                        ?.work
                        ?.async {
                            currentConversation()?.let {
                                notifications.setConversationEnabled(
                                    owner,
                                    it,
                                    action.value == "true",
                                )
                            }
                            projection.withLock { if (session.accepts(owner.key)) refreshLoaded(owner) }
                        }?.await()
                }
            }
        }
        viewModelScope.launch {
            kotlinx.coroutines.flow
                .combine(notifications.status, notifications.enabled) { status, enabled ->
                    status to
                        enabled
                }.collect { (status, enabled) ->
                    val owner = session.active.value
                    if (owner !=
                        null
                    ) {
                        session.withCurrent(owner.key) {
                            ui.update { currentUi ->
                                currentUi.copy(notificationStatus = status, notificationsEnabled = enabled)
                            }
                        }
                    } else {
                        ui.update { currentUi ->
                            currentUi.copy(notificationStatus = status, notificationsEnabled = false)
                        }
                    }
                }
        }
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
                synchronized(screenLock) {
                    screenCounter.incrementAndGet()
                    clearMetadata()
                }
                conversation = null
                logicalKey = null
                cache
                    .clear()
                if (owner == null) {
                    ui.value =
                        MessengerState(
                            backend =
                                ui.value.backend,
                            pendingReset = ui.value.pendingReset,
                            error = ui.value.error,
                        )
                } else {
                    try {
                        owner.work
                            .async {
                                val inbox = owner.client.inboxId()
                                if (!session.accepts(owner.key)) return@async
                                ui.value =
                                    MessengerState(
                                        screen = Screen.CONVERSATIONS,
                                        backend = owner.profile.backend,
                                        inbox = inbox,
                                        features = FeatureAvailability(metadata = true, notifications = notifications.configured),
                                        notificationStatus = notifications.status.value,
                                        notificationsEnabled = notifications.enabled.value,
                                    )
                                projection.withLock {
                                    beforeActiveRefresh(owner)
                                    refreshLoaded(owner)
                                }
                            }.await()
                    } catch (error: Throwable) {
                        if (error is CancellationException) {
                            currentCoroutineContext().ensureActive()
                        } else if (session.accepts(owner.key)) {
                            showError(error)
                        }
                    }
                }
            }
        }
        viewModelScope.launch {
            session.error.collect { error ->
                if (error != null) {
                    ui.update { currentUi -> currentUi.copy(error = error) }
                }
            }
        }
        viewModelScope.launch {
            session.readerError.collect {
                ui.update { currentUi -> currentUi.copy(readerError = it) }
            }
        }
        viewModelScope.launch {
            session.connection.collect {
                ui.update { currentUi ->
                    currentUi.copy(
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
        }
        startupRestoreJob =
            viewModelScope.launch {
                try {
                    if (sessionActionCounter.get() != 0L) return@launch
                    val saved = session.preferences.active()
                    if (saved != null && session.preferences.signedIn() && sessionActionCounter.get() == 0L) {
                        synchronized(screenLock) {
                            if (selectedBackend == BuildConfig.XMTP_BACKEND_URL.trim().trimEnd('/')) {
                                ui.update { currentUi -> currentUi.copy(backend = saved.backend) }
                            }
                        }
                    }
                    currentCoroutineContext().ensureActive()
                    if (sessionActionCounter.get() != 0L) return@launch
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
                            ui.update { currentUi ->
                                currentUi.copy(
                                    migrationRequired = true,
                                    migrationAccounts = accounts,
                                )
                            }
                        }
                    }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    val pending = session.preferences.reset() != null
                    if (sessionActionCounter.get() == 0L) {
                        ui.update { currentUi -> currentUi.copy(pendingReset = pending) }
                        showError(error)
                    }
                }
            }
    }

    suspend fun openPush(intent: android.content.Intent) {
        if (!notifications.configured || !intent.hasExtra("push-profile")) return
        val owner = session.restoreForPush() ?: return
        val route = notifications.tap(intent) ?: return
        if (route.profile != owner.key.profileId) return
        val inbox = session.withCurrent(owner.key) { owner.client.inboxId() } ?: return
        withTimeoutOrNull(10_000) { state.first { it.inbox == inbox } } ?: return
        session.withCurrent(owner.key) {
            if (route.conversation != null) {
                dispatch(MessengerAction.OpenConversation(route.conversation))
            } else {
                dispatch(MessengerAction.Navigate(Screen.CONVERSATIONS))
            }
        }
    }

    internal var beforeFeaturesUiUpdate: () -> Unit = {}

    fun setFeatures(value: FeatureAvailability) {
        beforeFeaturesUiUpdate()
        ui.update { currentUi -> currentUi.copy(features = value.copy(notifications = notifications.configured)) }
    }

    internal fun setAttachmentAvailability(
        owner: ActiveSession,
        token: Long,
        supported: Boolean,
        current: () -> Boolean,
    ): Boolean =
        onCurrentScreen(owner, token) {
            if (current()) {
                beforeFeaturesUiUpdate()
                ui.update { currentUi ->
                    currentUi.copy(features = currentUi.features.copy(attachments = supported))
                }
                true
            } else {
                false
            }
        } == true

    fun foreground(value: Boolean) {
        synchronized(screenLock) { foreground = value }
        if (value) {
            notifications.permissionChanged()
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
    internal var writeConsent: suspend (
        Conversation,
        ConsentState,
    ) -> Unit = { chat, value -> chat.updateConsentState(value) }
    internal var onConsentFinished: () -> Unit = {}
    internal var listGroupStateRead: suspend (Group) -> GroupState = { group -> group.state() }
    internal var historyPageRead: suspend (
        Conversation,
        ListMessagesOptions,
        MessageHistoryPosition?,
        MessageHistoryPosition?,
    ) -> MessageHistoryPage = { chat, options, before, after -> chat.historyPage(options, before, after) }

    private val backendProbeCounter = AtomicLong()
    private var backendProbeJob: Job? = null

    internal var inspectBackend: suspend (String) -> ServerConfiguration = { backend ->
        SDKClient.fetchServerConfiguration(BackendSource.Options(BackendOptions(url = backend)))
    }
    internal var onBackendProbeFinished: (Long, String) -> Unit = { _, _ -> }

    private fun inspect(backend: String) {
        val url = backend.trim().trimEnd('/')
        val attempt = backendProbeCounter.incrementAndGet()
        val token =
            synchronized(screenLock) {
                selectedBackend = url
                ui.update { currentUi -> currentUi.copy(credentialsRequiredFor = null) }
                screenGeneration
            }
        backendProbeJob?.cancel()
        backendProbeJob =
            viewModelScope.launch(Dispatchers.IO) {
                try {
                    delay(300)
                    val configuration = inspectBackend(validatedBackendUrl(url))
                    synchronized(screenLock) {
                        if (attempt == backendProbeCounter.get() && token == screenGeneration &&
                            ui.value.screen == Screen.START && selectedBackend == url
                        ) {
                            ui.update { currentUi ->
                                currentUi.copy(
                                    credentialsRequiredFor = url.takeIf { configuration.auth.enabled },
                                )
                            }
                        }
                    }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    // A failed capability query does not establish an authentication requirement.
                } finally {
                    onBackendProbeFinished(attempt, url)
                }
            }
    }

    internal var beforeActiveRefresh: suspend (ActiveSession) -> Unit = {}
    internal var beforeGroupWrite: suspend (MessengerAction, Int) -> Unit = { _, _ -> }
    internal var beforeQueuedAction: suspend (MessengerAction) -> Unit = {}
    internal var onQueuedActionFinished: (MessengerAction) -> Unit = {}
    internal var actionMessageRead: suspend (ActiveSession, String) -> Message? = { owner, id ->
        owner.client.conversations.getMessageById(id)
    }
    internal var recoveryRead: suspend (
        Conversation,
        ListMessagesOptions,
        MessageRecoveryPosition?,
        MessageRecoveryPosition?,
    ) -> MessageRecoveryPage = { chat, options, before, after ->
        chat.recoveryPage(options, before, after)
    }

    internal suspend fun awaitStartupRestore() {
        startupRestoreJob?.join()
    }

    fun dispatch(action: MessengerAction) {
        if (action is MessengerAction.InspectBackend) {
            inspect(action.backend)
            return
        }
        if (action is MessengerAction.Navigate) {
            navigate(
                action.screen,
            )
            return
        }
        if (action is MessengerAction.Reply) {
            ui.update { currentUi ->
                currentUi.copy(
                    replyTo =
                        currentUi.messages
                            .firstOrNull {
                                it.id == action.messageId && !it.deleted
                            }?.id,
                    replyPreview =
                        currentUi.messages
                            .firstOrNull {
                                it.id ==
                                    action.messageId && !it.deleted
                            }?.text
                            ?.lineSequence()
                            ?.firstOrNull(),
                )
            }
            return
        }
        val origin =
            synchronized(screenLock) {
                ActionOrigin(
                    session.active.value,
                    conversation,
                    ui.value.replyTo,
                    if (action is MessengerAction.OpenConversation) {
                        screenCounter.incrementAndGet()
                    } else {
                        screenGeneration
                    },
                )
            }
        val actionScope =
            if (action is MessengerAction
                    .Connect ||
                action is MessengerAction
                    .ResetLegacyAccount || action ==
                MessengerAction
                    .SignOut || action ==
                MessengerAction.DeleteAccount || action == MessengerAction.ResumeReset
            ) {
                viewModelScope
            } else {
                origin.owner?.work ?: viewModelScope
            }
        val replacesSession =
            action is MessengerAction
                .Connect ||
                action is MessengerAction
                    .ResetLegacyAccount || action ==
                MessengerAction
                    .SignOut || action ==
                MessengerAction.DeleteAccount || action == MessengerAction.ResumeReset
        val operationToken =
            if (replacesSession) {
                sessionActionCounter
                    .incrementAndGet()
            } else {
                sessionActionCounter
                    .get()
            }
        if (replacesSession) startupRestoreJob?.cancel()
        actionScope
            .launch(
                Dispatchers.IO,
            ) {
                val owner = origin.owner
                val token = origin.token
                try {
                    beforeQueuedAction(action)
                    if (replacesSession && operationToken != sessionActionCounter.get()) return@launch
                    when (action) {
                        is MessengerAction.Connect,
                        -> {
                            ui.update { currentUi ->
                                currentUi.copy(
                                    busy = true,
                                    error = null,
                                )
                            }
                            session
                                .connect(
                                    action.backend,
                                    action.credential,
                                    localAttachmentNetwork(action.backend),
                                )
                        }

                        MessengerAction.SignOut,
                        -> {
                            session
                                .signOut()
                        }

                        MessengerAction.DeleteAccount, MessengerAction.ResumeReset,
                        -> {
                            ui.update { currentUi -> currentUi.copy(pendingReset = true, busy = true, error = null) }
                            session
                                .deleteAccount()
                            if (operationToken != sessionActionCounter.get()) return@launch
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
                            if (operationToken != sessionActionCounter.get()) return@launch
                            ui.update { currentUi ->
                                currentUi.copy(
                                    migrationAccounts =
                                        currentUi
                                            .migrationAccounts -
                                            action.inboxId,
                                    migrationRequired =
                                        currentUi.migrationAccounts.size > 1,
                                )
                            }
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
                            if (action !is MessengerAction.OpenConversation && !valid(owner, token)) return@launch
                            when (action) {
                                is MessengerAction.InspectBackend -> {
                                    Unit
                                }

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
                                        ui.update { currentUi ->
                                            currentUi.copy(
                                                unknownTab =
                                                    action.unknown,
                                            )
                                        }
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

                                MessengerAction.LoadOlderRecovery,
                                MessengerAction.LatestRecovery,
                                -> {
                                    projection.withLock {
                                        if (!valid(owner, token)) return@withLock
                                        recoveryUpper =
                                            if (action == MessengerAction.LatestRecovery) null else recoveryNext
                                        refreshTimeline(owner, token, preserve = true)
                                    }
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
                                    val chat = origin.chat ?: return@launch
                                    var accepted = false
                                    try {
                                        onCurrentScreen(owner, token) {
                                            ui.update { currentUi ->
                                                currentUi.copy(error = null)
                                            }
                                        }
                                        requireOrigin(origin)
                                        val reply =
                                            origin.replyId?.let { id ->
                                                actionMessageRead(owner, id).also { parent ->
                                                    requireOrigin(origin)
                                                    require(
                                                        parent != null && parent.conversationId == chat.id(),
                                                    ) { "Reply parent unavailable" }
                                                }
                                            }
                                        sends
                                            .queue(
                                                owner.key,
                                                owner.client,
                                                chat,
                                                admission = { acceptsOrigin(origin) },
                                                onStored = { accepted = true },
                                                onAccepted = {
                                                    onCurrentScreen(owner, token) {
                                                        val sameReply = ui.value.replyTo == origin.replyId
                                                        val replyId = ui.value.replyTo.takeUnless { sameReply }
                                                        val preview = ui.value.replyPreview.takeUnless { sameReply }
                                                        ui.update { currentUi ->
                                                            currentUi.copy(
                                                                textSendResult =
                                                                    TextSendResult(
                                                                        action.requestId,
                                                                        chat.id(),
                                                                        true,
                                                                    ),
                                                                replyTo = replyId,
                                                                replyPreview = preview,
                                                            )
                                                        }
                                                    }
                                                },
                                                reconcile = {
                                                    merge(
                                                        owner,
                                                        token,
                                                        it,
                                                    )
                                                },
                                            ) {
                                                requireOrigin(origin)
                                                if (reply != null) {
                                                    reply.reply(
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
                                    } finally {
                                        if (!accepted) {
                                            onCurrentScreen(owner, token) {
                                                ui.update { currentUi ->
                                                    currentUi.copy(
                                                        textSendResult =
                                                            TextSendResult(
                                                                action.requestId,
                                                                chat.id(),
                                                                false,
                                                            ),
                                                    )
                                                }
                                            }
                                        }
                                    }
                                }

                                is MessengerAction.RetrySend,
                                -> {
                                    val chat = origin.chat ?: return@launch
                                    requireOrigin(origin)
                                    sends.retry(
                                        owner.key,
                                        owner.client,
                                        chat,
                                        action.messageId,
                                        admission = { acceptsOrigin(origin) },
                                    ) { message -> merge(owner, token, message) }
                                }

                                is MessengerAction.React,
                                -> {
                                    val chat = origin.chat ?: return@launch
                                    requireOrigin(origin)
                                    val message =
                                        actionMessageRead(owner, action.messageId) ?: error("Message unavailable")
                                    requireOrigin(origin)
                                    require(
                                        message.conversationId == chat.id(),
                                    ) { "Message belongs to another conversation" }
                                    sends
                                        .queue(
                                            owner.key,
                                            owner.client,
                                            chat,
                                            admission = { acceptsOrigin(origin) },
                                            reconcile = {
                                                merge(
                                                    owner,
                                                    token,
                                                    it,
                                                )
                                            },
                                        ) {
                                            requireOrigin(origin)
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
                                    val chat = origin.chat ?: return@launch
                                    requireOrigin(origin)
                                    val message =
                                        actionMessageRead(owner, action.messageId) ?: error("Message unavailable")
                                    requireOrigin(origin)
                                    require(
                                        message.conversationId == chat.id(),
                                    ) { "Message belongs to another conversation" }
                                    chat.deleteMessage(action.messageId)
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
                                    val chat = origin.chat ?: return@launch
                                    try {
                                        requireOrigin(origin)
                                        writeConsent(
                                            chat,
                                            if (action.allowed) ConsentState.ALLOWED else ConsentState.DENIED,
                                        )
                                        val current =
                                            session.withCurrent(owner.key) {
                                                synchronized(screenLock) {
                                                    if (token != screenGeneration || conversation?.id() != chat.id()) {
                                                        false
                                                    } else {
                                                        if (!action.allowed) navigate(Screen.CONVERSATIONS)
                                                        true
                                                    }
                                                }
                                            } ?: false
                                        if (!current) return@launch
                                        projection.withLock {
                                            if (session.accepts(owner.key)) refreshLoaded(owner)
                                        }
                                    } finally {
                                        onConsentFinished()
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
                                    mutateGroup(origin, action)
                                }
                            }
                        }
                    }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    if (operationToken ==
                        sessionActionCounter
                            .get() && (
                            replacesSession || owner == null || valid(owner, token)
                        )
                    ) {
                        showError(error)
                    }
                } finally {
                    onQueuedActionFinished(action)
                    if (operationToken ==
                        sessionActionCounter
                            .get() && (
                            replacesSession || owner == null || valid(owner, token)
                        )
                    ) {
                        ui.update { currentUi -> currentUi.copy(busy = false) }
                    }
                }
            }
    }

    private fun navigate(screen: Screen) {
        synchronized(screenLock) {
            screenCounter.incrementAndGet()
            clearMetadata()
            atNewest = false
            ui.update { currentUi ->
                currentUi.copy(
                    screen = screen,
                    error = null,
                )
            }
        }
        if (screen == Screen.CONVERSATIONS || screen ==
            Screen.CONVERSATION_SETTINGS || screen == Screen.GROUP_FIELDS || screen == Screen.MY_FIELDS
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
            ui.update { currentUi ->
                currentUi.copy(
                    error =
                        if (currentUi.pendingReset) {
                            "Local reset failed. ${error.message ?: "Cleanup is not complete."}"
                        } else {
                            error.toString()
                        },
                    busy = false,
                )
            }
        }
    }

    private suspend fun refreshUnknown(owner: ActiveSession) {
        sends.recoverAccepted(owner.key)
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
                            ) &&
                        !sends.knowsAccepted(it.draftId)
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
            ui.update { currentUi -> currentUi.copy(unknownSends = unknown) }
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
        refreshMetadata(owner)
        featureRefresh(
            owner,
            conversation,
        )
    }

    internal suspend fun refreshList(owner: ActiveSession) =
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
                                val groupState =
                                    (chat as? Conversation.Group)?.let { listGroupStateRead(it.group) }
                                val state = groupState?.common ?: conversationState(chat)
                                val title =
                                    when (chat) {
                                        is Conversation.Group,
                                        -> {
                                            checkNotNull(groupState)
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
                                    historyPageRead(
                                        chat,
                                        publishedSelection()
                                            .copy(limit = 1u),
                                        null,
                                        null,
                                    ).messages
                                        .firstOrNull()
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
                ui.update { currentUi -> currentUi.copy(conversations = rows) }
            }
        }

    internal var onOpenFinished: (String) -> Unit = {}
    private val openAttemptCounter = AtomicLong()
    internal var onOpenStarted: (Long, String) -> Unit = { _, _ -> }
    internal var onOpenAttemptFinished: (Long, String) -> Unit = { _, _ -> }

    private suspend fun open(
        owner: ActiveSession,
        id: String,
        token: Long,
    ) {
        val attempt = openAttemptCounter.incrementAndGet()
        onOpenStarted(attempt, id)
        try {
            performOpen(owner, id, token)
        } finally {
            onOpenAttemptFinished(attempt, id)
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
        val opened =
            session.withCurrent(owner.key) {
                synchronized(screenLock) {
                    if (token != screenGeneration) return@synchronized false
                    conversation = chat
                    logicalKey = key
                    synchronized(screenLock) {
                        atNewest = false
                        newestLoaded = false
                    }
                    nextBefore = null
                    recoveryUpper = null
                    recoveryNext = null
                    ui.update { currentUi ->
                        currentUi.copy(
                            screen = Screen.TIMELINE,
                            conversationId = id,
                            conversationTitle =
                                currentUi.conversations
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
                    }
                    true
                }
            } ?: false
        if (!opened) return
        projection.withLock {
            if (!valid(owner, token)) return
            val cached = cache.get(id)
            if (cached != null && anchor != null && cached.rows.any { it.id == anchor.messageId }) {
                onCurrentScreen(owner, token) {
                    val cachedRows = cached.rows.map { it.toRow(owner.client.inboxId()) }
                    ui.update { currentUi -> currentUi.copy(messages = cachedRows, anchor = anchor) }
                    nextBefore = cached.last
                }
                refreshTimeline(owner, token, true)
            } else if (anchor != null && !anchor.wasAtNewest) {
                restorePosition(owner, token, anchor)
            } else {
                refreshTimeline(owner, token, false)
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

    private fun MessengerState.withRecovery(page: PendingMessagePage): MessengerState {
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
            val row = owner.client.conversations.getMessageById(saved.messageId)
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
        val consent = conversationState(chat).consentState
        if (!valid(owner, token) || chat.id() != conversation?.id()) return
        onCurrentScreen(owner, token) {
            val window = cache.put(chat.id(), result, saved?.messageId)
            nextBefore = window.last
            newestLoaded = window.atNewest
            val position = anchorForWindow(owner, saved, window)
            ui.update { currentUi ->
                currentUi
                    .copy(
                        messages = timelineRows(owner, window.rows, queued.rows),
                        historyNotice = window.notice ?: if (changed || position.changed) "Position changed" else null,
                        hasOlder = window.hasOlder,
                        conversationUnknown = consent == ConsentState.UNKNOWN,
                        anchor = position.anchor,
                    ).withRecovery(queued)
                    .refreshReply()
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
                    .refreshReply()
            }
        }
    }

    private suspend fun restorePosition(
        owner: ActiveSession,
        token: Long,
        saved: ScrollAnchor,
    ) = refreshWindow(owner, token, saved)

    private fun MessengerState.refreshReply(): MessengerState {
        val parent = messages.firstOrNull { it.id == replyTo && !it.deleted }
        return copy(replyTo = parent?.id, replyPreview = parent?.text?.lineSequence()?.firstOrNull())
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
        if (!admitPosition(owner, token, false) { atNewest = action.atNewest }) return
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

    private fun <T> onCurrentScreen(
        owner: ActiveSession,
        token: Long,
        change: () -> T,
    ): T? =
        session.withCurrent(owner.key) {
            synchronized(screenLock) {
                if (token == screenGeneration) change() else null
            }
        }

    private fun admitPosition(
        owner: ActiveSession,
        token: Long,
        read: Boolean,
        change: () -> Unit,
    ): Boolean =
        onCurrentScreen(owner, token) {
            if (ui.value.screen == Screen.TIMELINE && (!read || (foreground && atNewest && newestLoaded))) {
                change()
                true
            } else {
                false
            }
        } ?: false

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
                        admit = { change -> admitPosition(owner, token, true, change) },
                    )
                if (valid(owner, token)) refreshUnread(owner, chat, key)
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
            ui.update { currentUi ->
                currentUi.copy(
                    conversations =
                        currentUi.conversations.map {
                            if (it.id == chat.id()) it.copy(unread = count) else it
                        },
                )
            }
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
        if (!valid(owner, token)) return
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

    private data class ActionOrigin(
        val owner: ActiveSession?,
        val chat: Conversation?,
        val replyId: String?,
        val token: Long,
    )

    private fun acceptsOrigin(origin: ActionOrigin): Boolean =
        synchronized(screenLock) {
            val owner = origin.owner ?: return@synchronized false
            valid(owner, origin.token) && origin.chat?.id() == conversation?.id()
        }

    private fun requireOrigin(origin: ActionOrigin) {
        if (!acceptsOrigin(origin)) throw CancellationException("Action scope changed")
    }

    private suspend fun mutateGroup(
        origin: ActionOrigin,
        action: MessengerAction,
    ) {
        val owner = origin.owner ?: return
        val token = origin.token
        val chat = origin.chat ?: return
        requireOrigin(origin)
        var writeIndex = 0

        suspend fun admitWrite() {
            beforeGroupWrite(action, writeIndex)
            if (writeIndex == 0) {
                requireOrigin(origin)
            } else if (!session.accepts(owner.key)) {
                throw CancellationException("Session changed during group action")
            }
            writeIndex += 1
        }
        try {
            if (action is MessengerAction.SetDisappearing) {
                require(
                    action.seconds >= 0,
                )
                admitWrite()
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
                        admitWrite()
                        group
                            .updateName(
                                action.name,
                            )
                        admitWrite()
                        group
                            .updateDescription(
                                action.description,
                            )
                    }

                    is MessengerAction.AddMember,
                    -> {
                        admitWrite()
                        group
                            .addMembers(
                                listOf(
                                    action.inboxId,
                                ),
                            )
                    }

                    is MessengerAction.RemoveMember,
                    -> {
                        admitWrite()
                        group
                            .removeMembers(
                                listOf(
                                    action.inboxId,
                                ),
                            )
                    }

                    is MessengerAction.SetAdmin,
                    -> {
                        admitWrite()
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
                        admitWrite()
                        group
                            .requestRemoval()
                    }

                    is MessengerAction.SetPreset,
                    -> {
                        applyStandardPreset(
                            group,
                            action.adminOnly,
                            beforeWrite = { admitWrite() },
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
            onCurrentScreen(owner, token) {
                ui.update { currentUi -> currentUi.copy(settings = settings) }
            }
        }
    }

    private fun clearMetadata() {
        metadataJob?.cancel()
        metadataJob = null
        metadataBinding = null
        metadataUi.value = MetadataEditorState(busy = true)
    }

    private suspend fun refreshMetadata(owner: ActiveSession) {
        if (!viewModelScope.isActive || ui.value.screen !in listOf(Screen.GROUP_FIELDS, Screen.MY_FIELDS)) return
        val chat = conversation ?: return
        val token = screenGeneration
        owner.work
            .async {
                metadataMutex.withLock {
                    if (!valid(owner, token)) return@withLock
                    try {
                        var binding = metadataBinding
                        if (binding?.key != owner.key ||
                            binding.token != token || binding.conversationId != chat.id()
                        ) {
                            val offered = reads.withPermit { owner.client.serverConfiguration().applicationComponents }
                            if (!valid(owner, token)) return@withLock
                            val controller =
                                MetadataEditorController(chat, owner.client.inboxId(), {
                                    val current = viewModelScope.isActive && acceptsScreen(owner.key, token)
                                    current && conversation?.id() == chat.id()
                                }, offered, reads)
                            binding = MetadataBinding(owner.key, token, chat.id(), controller)
                            onCurrentScreen(owner, token) {
                                metadataBinding = binding
                                metadataJob?.cancel()
                                metadataJob =
                                    viewModelScope.launch {
                                        controller.state.collect { state ->
                                            onCurrentScreen(owner, token) { metadataUi.value = state }
                                        }
                                    }
                            }
                        }
                        binding.controller.refresh()
                    } catch (error: Throwable) {
                        if (error is CancellationException) throw error
                        onCurrentScreen(owner, token) {
                            metadataUi.value =
                                metadataUi.value.copy(
                                    busy = false,
                                    error = "${error.javaClass.simpleName}: ${error.message ?: error}",
                                )
                        }
                    }
                }
            }.await()
    }

    fun editMetadata(edit: MetadataEdit) {
        if (!viewModelScope.isActive) return
        val binding = metadataBinding ?: return
        val owner = session.active.value ?: return
        if (!acceptsScreen(binding.key, binding.token) || owner.key != binding.key) return
        owner.work.launch { binding.controller.edit(edit) }
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
