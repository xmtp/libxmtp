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
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.ActiveSession
import org.xmtp.android.example.messenger.AppSession
import org.xmtp.android.example.messenger.SessionKey
import org.xmtp.android.example.messenger.conversationState
import org.xmtp.android.example.messenger.logicalConversationKey
import uniffi.xmtp_sdk.*

class NotificationController(context: Context, private val session: AppSession, private val transport: PushTransport = NotificationTransport(context)) : PushAdmission {
    private val context = context.applicationContext
    private val preferences = NotificationPreferences(this.context)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val registration = Mutex()
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
        val previous = session.beforeEnd
        session.beforeEnd = { owner ->
            try { previous(owner) } finally {
                registration.withLock {
                    registeredOwner = null; registeredToken = null
                    NotificationManagerCompat.from(this.context).cancelAll()
                    if (session.preferences.reset()?.profileId == owner.key.profileId) preferences.remove(owner.key.profileId)
                }
            }
        }
        scope.launch { session.active.collect { owner ->
            enabledState.value = false
            if (owner == null) state.value = if (configured) "Off" else "Off: Firebase is not configured"
            else owner.work.launch { refresh(owner) }
        } }
    }

    private fun permission() = (Build.VERSION.SDK_INT < 33 || ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) && NotificationManagerCompat.from(context).areNotificationsEnabled()
    private fun owner(key: PushOwner) = session.active.value?.takeIf { it.key.profileId == key.profile && it.key.generation == key.generation && session.accepts(it.key) }
    override fun current(): PushOwner? = session.active.value?.takeIf { session.accepts(it.key) }?.let { PushOwner(it.key.profileId, it.key.generation, it.client.installationIdBytes().joinToString("") { byte -> "%02x".format(byte.toInt() and 255) }) }
    override suspend fun enabled(owner: PushOwner) = configured && permission() && owner(owner) != null && session.preferences.signedIn() && session.preferences.reset() == null && preferences.enabled(owner.profile)
    override suspend fun conversation(owner: PushOwner, source: String): PushConversation? {
        val active = owner(owner) ?: return null
        val chat = active.client.conversations.getById(source) ?: return null
        val current = conversationState(chat)
        val logical = logicalConversationKey(chat, active.client.inboxId())
        return PushConversation(chat.id(), current.isActive && current.consentState != ConsentState.DENIED, current.notificationsEnabled && !preferences.muted(owner.profile, logical))
    }
    override fun postIfCurrent(owner: PushOwner, envelope: PushEnvelope, route: PushRoute): Boolean {
        val active = owner(owner) ?: return false
        if (!permission()) return false
        return session.withCurrent(active.key) {
            val manager = context.getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(NotificationChannel(CHANNEL, "Messages", NotificationManager.IMPORTANCE_DEFAULT))
            val intent = Intent(context, MainActivity::class.java).apply {
                data = Uri.Builder().scheme("xmtp-messenger").authority("push").appendPath(route.profile).appendPath(envelope.topic).appendPath(envelope.sequence.toString()).build()
                putExtra(PROFILE, route.profile)
                putExtra(CONVERSATION, route.conversation)
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP
            }
            val tap = PendingIntent.getActivity(context, 0, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
            val notification = NotificationCompat.Builder(context, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_dialog_email).setContentTitle("XMTP Messenger")
                .setContentText("You got a message.").setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
                .setPublicVersion(NotificationCompat.Builder(context, CHANNEL).setSmallIcon(android.R.drawable.ic_dialog_email).setContentTitle("XMTP Messenger").setContentText("You got a message.").build())
                .setContentIntent(tap).setAutoCancel(true).setOnlyAlertOnce(true).build()
            manager.notify(envelope.tag, 0, notification)
            true
        } ?: false
    }

    fun receive(data: Map<String, String>) {
        val active = session.active.value ?: return
        active.work.launch { try { handler.receive(data) } catch (error: Exception) { if (error is CancellationException) throw error } }
    }
    fun tokenChanged(value: String) {
        token = value
        session.active.value?.let { owner -> owner.work.launch { reconcile(owner) } }
    }
    fun permissionChanged() { session.active.value?.let { owner -> owner.work.launch { refresh(owner) } } }
    suspend fun refresh(owner: ActiveSession) {
        if (!session.accepts(owner.key)) return
        val enabled = configured && preferences.enabled(owner.key.profileId)
        if (!session.accepts(owner.key)) return
        enabledState.value = enabled
        if (enabled && permission()) transport.requestToken { value, error ->
            if (session.accepts(owner.key)) {
                if (value != null) tokenChanged(value)
                else state.value = "FCM token failed: ${error?.javaClass?.simpleName ?: "Unknown"}"
            }
        }
        reconcile(owner)
    }
    suspend fun setEnabled(owner: ActiveSession, enabled: Boolean) {
        if (!configured || !session.accepts(owner.key)) return
        preferences.setEnabled(owner.key.profileId, enabled)
        if (!session.accepts(owner.key)) return
        if (enabled && !permission() && Build.VERSION.SDK_INT >= 33) requestState.value += 1
        refresh(owner)
    }
    suspend fun setConversationEnabled(owner: ActiveSession, conversation: Conversation, enabled: Boolean) {
        if (!configured || !session.accepts(owner.key)) return
        val key = logicalConversationKey(conversation, owner.client.inboxId())
        if (!session.accepts(owner.key)) return
        conversation.setNotifications(if (enabled) NotificationOverride.ENABLED else NotificationOverride.DISABLED)
        if (!session.accepts(owner.key)) return
        preferences.setMuted(owner.key.profileId, key, !enabled)
        reconcile(owner)
    }
    private suspend fun reconcile(owner: ActiveSession) = registration.withLock {
        if (!configured || !session.accepts(owner.key)) return@withLock
        try {
            val appEnabled = preferences.enabled(owner.key.profileId)
            val hasPermission = permission()
            val registrationToken = token
            val current = owner.client.notificationState()
            val action = registrationAction(RegistrationInput(configured, session.preferences.signedIn() && session.preferences.reset() == null, hasPermission, appEnabled, registrationToken), registeredToken.takeIf { registeredOwner == owner.key }, current is NotificationState.Enabled)
            if (!session.accepts(owner.key)) return@withLock
            val result = when (action) {
                RegistrationAction.ENABLE -> owner.client.enableNotifications(NotificationConfig(channel = uniffi.xmtp_sdk.NotificationChannel.Fcm(checkNotNull(registrationToken)), consentStates = listOf(ConsentState.ALLOWED, ConsentState.UNKNOWN), includeWelcomes = true, includeSyncGroups = false, includeCommits = false)).also {
                    if (session.accepts(owner.key) && it is NotificationState.Enabled) { registeredOwner = owner.key; registeredToken = registrationToken }
                }
                RegistrationAction.DISABLE -> { owner.client.disableNotifications(); registeredOwner = null; registeredToken = null; NotificationState.Disabled }
                RegistrationAction.NONE -> current
            }
            if (session.accepts(owner.key)) state.value = when {
                !appEnabled -> "Off"
                !hasPermission -> "Off: Android permission is required"
                token == null -> "Waiting for FCM token"
                result is NotificationState.Failed && result.error == NotificationFailure.CHANNEL_NOT_CONFIGURED -> "Backend FCM channel is not configured"
                result is NotificationState.Failed -> "Notification registration failed: ${result.error}"
                result is NotificationState.Enabled -> "On"
                else -> "Off"
            }
        } catch (error: Exception) {
            if (error is CancellationException) throw error
            if (session.accepts(owner.key)) state.value = if (error is XmtpException.ChannelNotConfigured) "Backend FCM channel is not configured" else "Notification registration failed: ${error.javaClass.simpleName}"
        }
    }
    suspend fun tap(intent: Intent): PushRoute? {
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
    companion object {
        private const val CHANNEL = "messenger-messages"
        private const val PROFILE = "push-profile"
        private const val CONVERSATION = "push-conversation"
    }
}
