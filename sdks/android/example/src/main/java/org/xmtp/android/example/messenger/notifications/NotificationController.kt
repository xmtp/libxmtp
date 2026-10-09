package org.xmtp.android.example.messenger.notifications

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.ActiveSession
import org.xmtp.android.example.messenger.AppSession
import org.xmtp.android.example.messenger.SessionKey
import org.xmtp.android.example.messenger.conversationState
import org.xmtp.android.example.messenger.logicalConversationKey
import uniffi.xmtp_sdk.*
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong

class NotificationController(
    context: Context,
    private val session: AppSession,
    private val transport: PushTransport = NotificationTransport(context),
) : PushAdmission {
    private val context = context.applicationContext
    private val preferences = NotificationPreferences(this.context)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val registration = Mutex()
    private val privacyRevision = AtomicLong()
    private val pendingPrivacyWrites = AtomicInteger()
    private var registeredOwner: SessionKey? = null
    private var registeredToken: String? = null

    @Volatile private var token: String? = null
    private val state = MutableStateFlow(if (transport.configured) "Off" else "Off: Firebase is not configured")
    val status: StateFlow<String> = state
    private val enabledState = MutableStateFlow(false)
    val enabled: StateFlow<Boolean> = enabledState
    private val requestState = MutableStateFlow(0L)
    val permissionRequest: StateFlow<Long> = requestState
    val configured get() = transport.configured
    private val handler = PushHandler(this)

    init {
        val previousAdmissionChange = session.onNotificationAdmissionChanged
        session.onNotificationAdmissionChanged = { owner ->
            previousAdmissionChange(owner)
            session.withCurrent(owner.key) { privacyRevision.incrementAndGet() }
        }
        session.unregisterNotifications = { client ->
            if (configured) registration.withLock { disableRegistration(client) }
        }
        session.needsNotificationPreflight =
            { profile -> !configured || !permission() || !preferences.enabled(profile) }
        val previousStop = session.stopStoredNotifications
        session.stopStoredNotifications = { owner ->
            previousStop(owner)
            if (owner.client.notificationState() !is NotificationState.Disabled) {
                try {
                    withTimeout(5_000) { disableRegistration(owner.client) }
                } catch (error: Exception) {
                    if (error is CancellationException) currentCoroutineContext().ensureActive()
                }
                check(owner.client.notificationState() is NotificationState.Disabled) {
                    "Cannot turn stored notifications off"
                }
            }
        }
        val previousRemoval = session.beforeProfileRemoval
        session.beforeProfileRemoval = { profile ->
            previousRemoval(profile)
            preferences.remove(profile)
        }
        val previous = session.beforeEnd
        session.beforeEnd = { owner ->
            try {
                previous(owner)
            } finally {
                registration.withLock {
                    // Feature work has stopped. A late enable cannot follow this unregister.
                    registeredOwner = null
                    registeredToken = null
                    NotificationManagerCompat.from(this.context).cancelAll()
                }
            }
        }
        scope.launch {
            session.active.collect { owner ->
                enabledState.value = false
                if (owner == null) {
                    requestState.value = 0
                    state.value = if (configured) "Off" else "Off: Firebase is not configured"
                } else {
                    owner.work.launch { refresh(owner) }
                }
            }
        }
    }

    internal var permissionGranted: () -> Boolean = { platformPermission() }
    internal var postNotification: (
        PushEnvelope,
        PushRoute,
        android.app.Notification,
    ) -> Boolean = {
        envelope,
        _,
        notification,
        ->
        NotificationPublisher.post(context, envelope.tag, notification)
    }

    internal var readConversationState: suspend (Conversation) -> ConversationState = ::conversationState
    internal var enableRegistration: suspend (
        SDKClient,
        NotificationConfig,
    ) -> NotificationState = {
        client,
        config,
        ->
        client.enableNotifications(config)
    }
    internal var readRegistrationState: (SDKClient) -> NotificationState = { client -> client.notificationState() }
    internal var disableRegistration: suspend (SDKClient) -> Unit = { client -> client.disableNotifications() }

    private fun permission() = permissionGranted()

    private fun platformPermission() =
        (
            Build.VERSION.SDK_INT < 33 ||
                ContextCompat.checkSelfPermission(
                    context,
                    Manifest.permission.POST_NOTIFICATIONS,
                ) == PackageManager.PERMISSION_GRANTED
        ) &&
            NotificationManagerCompat
                .from(context)
                .areNotificationsEnabled()

    private fun owner(key: PushOwner) =
        session.active.value?.takeIf {
            it.key.profileId == key.profile &&
                it.key.generation == key.generation &&
                session.accepts(it.key)
        }

    override fun current(): PushOwner? =
        session.active.value?.let { active ->
            session.withCurrent(active.key) {
                PushOwner(
                    active.key.profileId,
                    active.key.generation,
                    active.client
                        .installationIdBytes()
                        .joinToString("") { byte -> "%02x".format(byte.toInt() and 255) },
                )
            }
        }

    override suspend fun enabled(owner: PushOwner) =
        configured &&
            permission() &&
            owner(owner) != null &&
            session.preferences.signedIn() &&
            session.preferences.reset() == null &&
            preferences.enabled(owner.profile)

    override suspend fun conversation(
        owner: PushOwner,
        source: String,
    ): PushConversation? {
        val active = owner(owner) ?: return null
        val chat = active.client.conversations.getById(source) ?: return null
        val current = readConversationState(chat)
        val logical = logicalConversationKey(chat, active.client.inboxId())
        return PushConversation(
            chat.id(),
            current.isActive &&
                current.consentState != ConsentState.DENIED,
            current.notificationsEnabled &&
                !preferences.muted(
                    owner.profile,
                    logical,
                ),
        )
    }

    internal var beforeFinalAdmission: suspend () -> Unit = {}

    override suspend fun postIfCurrent(
        owner: PushOwner,
        envelope: PushEnvelope,
        route: PushRoute,
    ): Boolean {
        beforeFinalAdmission()
        val active = owner(owner) ?: return false
        if (pendingPrivacyWrites.get() != 0) return false
        val revision = privacyRevision.get()
        if (!enabled(owner)) return false
        if (envelope.kind == PushKind.GROUP) {
            val fresh = conversation(owner, envelope.identifier) ?: return false
            if (!fresh.allowed || !fresh.enabled || fresh.route != route.conversation) return false
        } else if (envelope.identifier != owner.installation || route.conversation != null) {
            return false
        }
        if (!permission()) return false
        return session.withCurrent(active.key) {
            if (pendingPrivacyWrites.get() != 0 || privacyRevision.get() != revision) return@withCurrent false
            val manager = context.getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(
                NotificationChannel(
                    CHANNEL,
                    "Messages",
                    NotificationManager.IMPORTANCE_DEFAULT,
                ),
            )
            val intent =
                Intent(context, MainActivity::class.java).apply {
                    data =
                        Uri
                            .Builder()
                            .scheme("xmtp-messenger")
                            .authority("push")
                            .appendPath(route.profile)
                            .appendPath(envelope.topic)
                            .appendPath(envelope.sequence.toString())
                            .build()
                    putExtra(PROFILE, route.profile)
                    putExtra(CONVERSATION, route.conversation)
                    flags = tapFlags()
                }
            val tap =
                PendingIntent.getActivity(
                    context,
                    0,
                    intent,
                    PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
                )
            val notification =
                NotificationCompat
                    .Builder(context, CHANNEL)
                    .setSmallIcon(android.R.drawable.ic_dialog_email)
                    .setContentTitle("XMTP Messenger")
                    .setContentText("You got a message.")
                    .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
                    .setPublicVersion(
                        NotificationCompat
                            .Builder(
                                context,
                                CHANNEL,
                            ).setSmallIcon(android.R.drawable.ic_dialog_email)
                            .setContentTitle("XMTP Messenger")
                            .setContentText("You got a message.")
                            .build(),
                    ).setContentIntent(tap)
                    .setAutoCancel(true)
                    .setOnlyAlertOnce(true)
                    .build()
            postNotification(envelope, route, notification)
        } ?: false
    }

    suspend fun receive(data: Map<String, String>): Boolean =
        try {
            receiveLocal(data)
        } catch (error: Exception) {
            if (error is CancellationException) currentCoroutineContext().ensureActive()
            false
        }

    private suspend fun receiveLocal(data: Map<String, String>): Boolean {
        if (!configured || !permission() || parsePush(data) == null) return false
        val profile = session.preferences.active() ?: return false
        if (!session.preferences.signedIn() ||
            session.preferences.reset() != null ||
            !preferences.enabled(profile.id)
        ) {
            return false
        }
        val active = session.restoreForPush() ?: return false
        return active.work.async { handler.receive(data) }.await()
    }

    fun tokenChanged(value: String) {
        token = value
        session.active.value?.let { owner -> owner.work.launch { reconcile(owner) } }
    }

    fun permissionRequested() {
        requestState.value = 0
    }

    fun permissionChanged() {
        session.active.value?.let { owner -> owner.work.launch { refresh(owner) } }
    }

    suspend fun refreshStatus(owner: ActiveSession) {
        if (!configured || !session.accepts(owner.key)) return
        val appEnabled = preferences.enabled(owner.key.profileId)
        val current = readRegistrationState(owner.client)
        session.withCurrent(owner.key) { state.value = statusText(appEnabled, permission(), current) }
    }

    private fun statusText(
        appEnabled: Boolean,
        hasPermission: Boolean,
        result: NotificationState,
    ): String =
        when {
            !appEnabled -> {
                "Off"
            }

            !hasPermission -> {
                "Off: Android permission is required"
            }

            token == null -> {
                "Waiting for FCM token"
            }

            result is NotificationState.Failed -> {
                if (result.error == NotificationFailure.CHANNEL_NOT_CONFIGURED) {
                    "Backend FCM channel is not configured"
                } else {
                    "Notification registration failed: ${result.error}"
                }
            }

            result is NotificationState.Enabled -> {
                "On"
            }

            else -> {
                "Off"
            }
        }

    fun close() {
        scope.coroutineContext[kotlinx.coroutines.Job]?.cancel()
    }

    suspend fun refresh(owner: ActiveSession) {
        if (!session.accepts(owner.key)) return
        val enabled = configured && preferences.enabled(owner.key.profileId)
        if (session.withCurrent(owner.key) {
                enabledState.value = enabled
                true
            } != true
        ) {
            return
        }
        if (enabled && permission()) {
            transport.requestToken { value, error ->
                session.withCurrent(owner.key) {
                    if (value != null) {
                        tokenChanged(value)
                    } else {
                        state.value = "FCM token failed: ${error?.javaClass?.simpleName ?: "Unknown"}"
                    }
                }
            }
        }
        reconcile(owner)
    }

    internal var beforePrivacyWrite: suspend (String) -> Unit = {}

    private fun beginPrivacyWrite() {
        pendingPrivacyWrites.incrementAndGet()
        privacyRevision.incrementAndGet()
    }

    private fun endPrivacyWrite() {
        privacyRevision.incrementAndGet()
        pendingPrivacyWrites.decrementAndGet()
    }

    internal suspend fun <T> withPrivacyMutation(
        owner: ActiveSession,
        block: suspend () -> T,
    ): T? {
        if (session.withCurrent(owner.key) {
                beginPrivacyWrite()
                true
            } != true
        ) {
            return null
        }
        return try {
            withContext(NonCancellable) {
                beforePrivacyWrite("sdk")
                block()
            }
        } finally {
            endPrivacyWrite()
        }
    }

    suspend fun setEnabled(
        owner: ActiveSession,
        enabled: Boolean,
    ) {
        if (!configured || session.withCurrent(owner.key) {
                beginPrivacyWrite()
                true
            } != true
        ) {
            return
        }
        try {
            withContext(NonCancellable) {
                beforePrivacyWrite("app")
                if (!preferences.setEnabled(
                        owner.key.profileId,
                        enabled,
                    ) { change -> session.admit(owner.key, change) }
                ) {
                    return@withContext
                }
                if (!session.accepts(owner.key)) return@withContext
                if (enabled && !permission() && Build.VERSION.SDK_INT >= 33) {
                    session.withCurrent(owner.key) {
                        enabledState.value = true
                        requestState.value += 1
                    }
                }
            }
            refresh(owner)
        } finally {
            endPrivacyWrite()
        }
    }

    internal var beforeConversationPreferenceLookup: suspend (ActiveSession, Conversation) -> Unit = { _, _ -> }
    internal var beforeConversationNotificationWrite: (Conversation) -> Unit = {}

    suspend fun setConversationEnabled(
        owner: ActiveSession,
        conversation: Conversation,
        enabled: Boolean,
        admit: (() -> Unit) -> Boolean = { change -> session.admit(owner.key, change) },
    ) {
        if (!configured || !admit {}) return
        beforeConversationPreferenceLookup(owner, conversation)
        val key = logicalConversationKey(conversation, owner.client.inboxId())
        if (!admit { beginPrivacyWrite() }) return
        try {
            withContext(NonCancellable) {
                beforePrivacyWrite("conversation")
                beforeConversationNotificationWrite(conversation)
                conversation.setNotifications(
                    if (enabled) NotificationOverride.ENABLED else NotificationOverride.DISABLED,
                )
                if (!session.accepts(owner.key)) return@withContext
                if (!preferences.setMuted(owner.key.profileId, key, !enabled, admit)) return@withContext
            }
            reconcile(owner)
        } finally {
            endPrivacyWrite()
        }
    }

    private suspend fun reconcile(owner: ActiveSession) =
        registration.withLock {
            if (!configured || !session.accepts(owner.key)) return@withLock
            try {
                val appEnabled = preferences.enabled(owner.key.profileId)
                val hasPermission = permission()
                val registrationToken = token
                val current = readRegistrationState(owner.client)
                val action =
                    registrationAction(
                        RegistrationInput(
                            configured,
                            session.preferences.signedIn() &&
                                session.preferences.reset() == null,
                            hasPermission,
                            appEnabled,
                            registrationToken,
                        ),
                        registeredToken.takeIf { registeredOwner == owner.key },
                        current is NotificationState.Enabled,
                    )
                if (!session.accepts(owner.key)) return@withLock
                val result =
                    when (action) {
                        RegistrationAction.ENABLE -> {
                            val channel = uniffi.xmtp_sdk.NotificationChannel.Fcm(checkNotNull(registrationToken))
                            val config =
                                NotificationConfig(
                                    channel = channel,
                                    consentStates = listOf(ConsentState.ALLOWED, ConsentState.UNKNOWN),
                                    includeWelcomes = true,
                                    includeSyncGroups = false,
                                    includeCommits = false,
                                )
                            enableRegistration(owner.client, config).also {
                                if (session.accepts(owner.key) && it is NotificationState.Enabled) {
                                    registeredOwner = owner.key
                                    registeredToken = registrationToken
                                }
                            }
                        }

                        RegistrationAction.DISABLE -> {
                            disableRegistration(owner.client)
                            registeredOwner = null
                            registeredToken = null
                            NotificationState.Disabled
                        }

                        RegistrationAction.NONE -> {
                            current
                        }
                    }
                session.withCurrent(owner.key) { state.value = statusText(appEnabled, hasPermission, result) }
            } catch (error: Exception) {
                if (error is CancellationException) throw error
                session.withCurrent(owner.key) {
                    state.value =
                        if (error is XmtpException.ChannelNotConfigured) {
                            "Backend FCM channel is not configured"
                        } else {
                            "Notification registration failed: ${error.javaClass.simpleName}"
                        }
                }
            }
        }

    suspend fun tap(intent: Intent): PushRoute? {
        val active = session.active.value ?: return null
        return active.work.async { tapOwned(intent) }.await()
    }

    private suspend fun tapOwned(intent: Intent): PushRoute? {
        val profile = intent.getStringExtra(PROFILE) ?: return null
        val active = current()?.takeIf { it.profile == profile } ?: return null
        if (!enabled(active)) return null
        val conversation = intent.getStringExtra(CONVERSATION)
        if (conversation != null) {
            val chat = conversation(active, conversation) ?: return null
            if (!chat.allowed || !chat.enabled || current() != active) return null
            return PushRoute(profile, chat.route)
        }
        return PushRoute(profile, null).takeIf { current() == active }
    }

    private fun tapFlags() =
        Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP

    companion object {
        private const val CHANNEL = "messenger-messages"
        private const val PROFILE = "push-profile"
        private const val CONVERSATION = "push-conversation"
    }
}
