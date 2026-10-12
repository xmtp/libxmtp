package org.xmtp.android.example.messenger.app

import android.content.Context
import android.content.Intent
import androidx.lifecycle.SavedStateHandle
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.sync.Semaphore
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.messenger.data.xmtp.*
import org.xmtp.android.example.messenger.feature.AppFeaturePreferences
import org.xmtp.android.example.messenger.feature.SecureComposerDrafts
import org.xmtp.android.example.messenger.feature.attachments.AttachmentController
import org.xmtp.android.example.messenger.feature.chat.ChatController
import org.xmtp.android.example.messenger.feature.connect.ConnectController
import org.xmtp.android.example.messenger.feature.conversations.ConversationsController
import org.xmtp.android.example.messenger.feature.conversations.CreateConversationController
import org.xmtp.android.example.messenger.feature.group.GroupController
import org.xmtp.android.example.messenger.feature.settings.SettingsController
import org.xmtp.android.example.messenger.notifications.*
import org.xmtp.android.example.messenger.platform.NotificationPermissionOwner
import org.xmtp.android.example.messenger.session.SessionObservers
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.attachments.AttachmentAction
import org.xmtp.android.example.shared.feature.conversations.CreateConversationAction
import org.xmtp.android.example.shared.feature.conversations.CreateStep
import uniffi.xmtp_sdk.ConsentState
import java.io.Closeable
import java.io.File

/** Constructs the process owners before any session command can run. */
internal class AppGraph(
    context: Context,
    sessionObservers: SessionObservers = SessionObservers(),
    notificationTransport: PushTransport = NotificationTransport(context),
    private val conversationQueries: ConversationQueries = XmtpQueries(),
    private val chatQueries: ChatQueries = XmtpQueries(),
    backendProbe: BackendProbe = XmtpBackendProbe(),
) : Closeable {
    private val context = context.applicationContext
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val reads = Semaphore(4)
    private val ready = MutableStateFlow<SessionKey?>(null)
    private val failure = MutableStateFlow<AppFailure?>(null)
    val navigationFailure: StateFlow<AppFailure?> = failure.asStateFlow()
    val destinations = RouteOwner { session }
    lateinit var notifications: NotificationController
        private set
    lateinit var sends: SendCoordinator
        private set
    lateinit var connect: ConnectController
        private set
    lateinit var conversations: ConversationsController
        private set
    lateinit var chat: ChatController
        private set
    lateinit var group: GroupController
        private set
    lateinit var settings: SettingsController
        private set
    lateinit var createConversation: CreateConversationController
        private set
    lateinit var attachments: AttachmentController
        private set
    lateinit var projection: FeatureMessageHandler
        private set

    val session: AppSession =
        AppSession(
            this.context,
            lifecycleFactory = { owner ->
                notifications = NotificationController(this.context, owner, notificationTransport)
                sessionObservers.decorateLifecycle(notifications.lifecycle)
            },
            projectionFactory = { owner ->
                val preferences = AppFeaturePreferences(this.context, owner, owner.secrets)
                val composerDrafts = SecureComposerDrafts(owner, owner.secrets)
                sends = SendCoordinator(owner.preferences, owner::accepts, owner::admit)
                connect =
                    ConnectController(
                        owner,
                        owner,
                        backendProbe,
                        preferences,
                        destinations,
                        scope,
                        BuildConfig.XMTP_BACKEND_URL,
                        ::legacyAccounts,
                    )
                conversations =
                    ConversationsController(
                        owner,
                        conversationQueries,
                        chatQueries,
                        preferences,
                        notifications,
                        reads,
                        destinations,
                        scope,
                    )
                chat =
                    ChatController(
                        owner,
                        chatQueries,
                        conversationQueries,
                        conversations,
                        composerDrafts,
                        sends,
                        reads,
                        destinations,
                        scope,
                    )
                group =
                    GroupController(
                        owner,
                        conversationQueries,
                        chatQueries,
                        preferences,
                        notifications,
                        reads,
                        destinations,
                        scope,
                    )
                createConversation = CreateConversationController(owner, conversationQueries, reads, destinations)
                settings =
                    SettingsController(
                        owner,
                        owner,
                        notifications,
                        sends,
                        preferences,
                        connect,
                        destinations,
                        scope,
                        composerDrafts,
                    )
                attachments = AttachmentController(this.context, owner, sends, destinations, chat, scope)
                projection =
                    FeatureMessageHandler(
                        owner,
                        conversations,
                        chat,
                        group,
                        settings,
                        createConversation,
                        attachments,
                        notifications,
                        scope,
                    )
                sessionObservers.decorateProjection(projection)
            },
        )
    val notificationPermission = NotificationPermissionOwner(session, notifications, settings, destinations)
    private var started = false
    private var foreground = false

    fun bindSavedState(
        handle: SavedStateHandle,
        ownerScope: CoroutineScope,
    ): Closeable = destinations.bindSavedState(handle, ownerScope)

    fun start() {
        if (started) return
        started = true
        projection.start()
        scope.launch {
            session.active.collectLatest { owner ->
                if (!destinations.sessionChanged(owner)) return@collectLatest
                ready.value = null
                failure.value = null
                try {
                    projection.activate(owner)
                    if (owner != null && session.accepts(owner.key)) {
                        destinations.restore(owner) { route -> canRestore(owner, route) }
                        if (session.accepts(owner.key)) ready.value = owner.key
                    }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    failure.value =
                        AppFailure("Cannot open your chats. Try again.", "session-view", FailureRecovery.RETRY)
                    if (owner != null && session.accepts(owner.key)) {
                        destinations.restore(owner) { false }
                        ready.value = owner.key
                    }
                }
            }
        }
        scope.launch {
            combine(ready, destinations.state) { key, destination -> key to destination }
                .collectLatest { (key, destination) ->
                    chat.destinationChanged()
                    val owner =
                        session.active.value?.takeIf { it.key == key && session.accepts(it.key) }
                            ?: return@collectLatest
                    chat.foreground(foreground && destination.route is AppRoute.Chat)
                    try {
                        owner.work
                            .async {
                                if (destinations.state.value != destination) return@async
                                when (val route = destination.route) {
                                    is AppRoute.Chat -> {
                                        chat.open(route.conversationId)
                                    }

                                    is AppRoute.ConversationDetails -> {
                                        chat.open(route.conversationId)
                                        group.open(route.conversationId)
                                    }

                                    is AppRoute.GroupFields -> {
                                        chat.open(route.conversationId)
                                        group.open(route.conversationId)
                                    }

                                    is AppRoute.OwnFields -> {
                                        chat.open(route.conversationId)
                                        group.open(route.conversationId, ownFields = true)
                                    }

                                    AppRoute.CreateConversation -> {
                                        createConversation.open()
                                    }

                                    AppRoute.Settings -> {
                                        settings.refresh(owner)
                                    }

                                    AppRoute.AttachmentRecovery -> {
                                        attachments.refresh(owner, destination.token)
                                    }

                                    AppRoute.Chats, AppRoute.Requests -> {
                                        conversations.showRequests(route == AppRoute.Requests)
                                        conversations.refresh(owner)
                                    }

                                    AppRoute.Connect -> {
                                        Unit
                                    }
                                }
                            }.await()
                        chat.foreground(foreground && destination.route is AppRoute.Chat)
                    } catch (error: Throwable) {
                        if (error is CancellationException) throw error
                        if (session.accepts(owner.key) && destinations.state.value == destination) {
                            failure.value =
                                AppFailure("Cannot open this screen. Try again.", "route", FailureRecovery.RETRY)
                        }
                    }
                }
        }
        attachments.start()
        connect.start()
    }

    private suspend fun canRestore(
        owner: ActiveSession,
        route: AppRoute,
    ): Boolean {
        val id = RouteOwner.conversationId(route) ?: return route != AppRoute.Connect
        val conversation = conversationQueries.find(owner, id) ?: return false
        return conversationQueries.state(conversation).consentState != ConsentState.DENIED
    }

    suspend fun openPush(intent: Intent) {
        connect.awaitStartupRestore()
        val owner = session.active.value ?: return
        combine(ready, session.active) { key, active -> key == owner.key || active?.key != owner.key }
            .first { it }
        if (ready.value != owner.key || !session.accepts(owner.key)) return
        val origin = destinations.capture(owner)
        val route = notifications.tap(intent) ?: return
        if (route.profile != owner.key.profileId) return
        destinations.admit(origin) {
            destinations.navigate(route.conversation?.let { AppRoute.Chat(it) } ?: AppRoute.Chats)
        }
    }

    fun foreground(value: Boolean) {
        foreground = value
        chat.foreground(value && destinations.state.value.route is AppRoute.Chat)
    }

    fun back(): Boolean {
        if (attachments.dismissChoiceSheet()) return true
        if (chat.dismissTopSurface()) return true
        if (destinations.state.value.route == AppRoute.CreateConversation &&
            createConversation.state.value.step == CreateStep.REVIEW
        ) {
            createConversation.dispatch(CreateConversationAction.Back)
            return true
        }
        return destinations.back()
    }

    fun dispatch(action: ShellAction) {
        failure.value = null
        when (action) {
            is ShellAction.Navigate -> {
                destinations.navigate(action.route)
            }

            ShellAction.Back -> {
                back()
            }

            ShellAction.RetryReader -> {
                scope.launch { session.retryReader() }
            }

            ShellAction.Refresh -> {
                val owner = session.active.value ?: return
                owner.work.launch {
                    try {
                        projection.invalidate(owner)
                    } catch (error: Throwable) {
                        if (error is CancellationException) throw error
                        if (session.accepts(owner.key)) {
                            failure.value =
                                AppFailure("Cannot refresh this screen. Try again.", "refresh", FailureRecovery.RETRY)
                        }
                    }
                }
            }
        }
    }

    fun dispatchAttachment(action: AttachmentAction) = attachments.dispatch(action)

    override fun close() {
        scope.cancel()
        notifications.close()
    }

    private fun legacyAccounts(): List<String> =
        File(context.filesDir, "xmtp_db")
            .listFiles()
            ?.filter { it.name.matches(Regex("xmtp-local-[0-9a-f]{64}\\.db3")) }
            ?.map { it.name.removePrefix("xmtp-local-").removeSuffix(".db3") }
            .orEmpty()
}
