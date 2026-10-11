package org.xmtp.android.example.messenger.app

import androidx.lifecycle.SavedStateHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import org.xmtp.android.example.messenger.ActiveSession
import org.xmtp.android.example.messenger.AppSession
import org.xmtp.android.example.messenger.SessionKey
import org.xmtp.android.example.shared.AppRoute
import java.io.Closeable

/** Owns navigation identity. Saved state contains route and profile IDs only. */
class RouteOwner(
    private val session: () -> AppSession,
) : DestinationScope {
    private val mutableState = MutableStateFlow(DestinationState())
    override val state: StateFlow<DestinationState> = mutableState.asStateFlow()
    private val backStack = mutableListOf<AppRoute>()
    private var restored: SavedRoute? = null
    private var savedStateRead = false
    private var boundSessionKey: SessionKey? = null
    private var restoreResolvedFor: SessionKey? = null
    private var pendingNavigation: PendingNavigation? = null

    override fun capture(owner: ActiveSession): DestinationOrigin =
        session().withStateLock { DestinationOrigin(owner.key, mutableState.value.token) }

    override fun accepts(origin: DestinationOrigin): Boolean =
        session().withCurrent(origin.sessionKey) {
            session().withStateLock {
                boundSessionKey == origin.sessionKey && mutableState.value.token == origin.destinationToken
            }
        } == true

    override fun admit(
        origin: DestinationOrigin,
        change: () -> Unit,
    ): Boolean {
        var changed = false
        session().admit(origin.sessionKey) {
            session().withStateLock {
                if (boundSessionKey == origin.sessionKey && mutableState.value.token == origin.destinationToken) {
                    change()
                    changed = true
                }
            }
        }
        return changed
    }

    override fun navigate(route: AppRoute) {
        val current = session()
        while (true) {
            val owner = current.active.value
            if (owner == null) {
                val applied =
                    session().withStateLock {
                        if (current.active.value != null) return@withStateLock false
                        pendingNavigation = null
                        navigateLocked(route)
                        true
                    }
                if (applied) return
            } else {
                current.withCurrent(owner.key) {
                    session().withStateLock {
                        if (current.active.value?.key != owner.key) return@withStateLock
                        if (boundSessionKey != owner.key) {
                            // Activation still owns binding. Keep the latest request for this generation.
                            restored = null
                            pendingNavigation = PendingNavigation(owner.key, route)
                        } else {
                            pendingNavigation = null
                            navigateLocked(route)
                        }
                    }
                }
                return
            }
        }
    }

    private fun navigateLocked(route: AppRoute) {
        restored = null
        restoreResolvedFor = boundSessionKey
        val previous = mutableState.value
        if (route == AppRoute.Connect || route == AppRoute.Chats) {
            backStack.clear()
        } else if (previous.route != AppRoute.Connect && previous.route != route) {
            backStack.add(previous.route)
        }
        mutableState.value = DestinationState(route, previous.token + 1)
    }

    fun back(): Boolean =
        session().withStateLock {
            if (pendingNavigation != null) {
                pendingNavigation = null
                return@withStateLock true
            }
            val previous = mutableState.value
            if (previous.route == AppRoute.Connect || previous.route == AppRoute.Chats) return@withStateLock false
            restored = null
            restoreResolvedFor = boundSessionKey
            val route = backStack.removeLastOrNull() ?: parent(previous.route)
            mutableState.value = DestinationState(route, previous.token + 1)
            true
        }

    fun clear() {
        session().withStateLock {
            restored = null
            pendingNavigation = null
            boundSessionKey = null
            restoreResolvedFor = null
            backStack.clear()
            mutableState.value = DestinationState(AppRoute.Connect, mutableState.value.token + 1)
        }
    }

    /** An initial signed-out observation must not discard a saved route. */
    fun sessionChanged(owner: ActiveSession?): Boolean {
        if (owner == null) {
            return session().withStateLock {
                if (session().active.value != null) return@withStateLock false
                pendingNavigation = null
                if (boundSessionKey == null) return@withStateLock false
                restored = null
                boundSessionKey = null
                restoreResolvedFor = null
                backStack.clear()
                mutableState.value = DestinationState(AppRoute.Connect, mutableState.value.token + 1)
                true
            }
        }
        return session().withCurrent(owner.key) {
            if (session().active.value?.key != owner.key) return@withCurrent false
            session().withStateLock {
                if (boundSessionKey == owner.key) return@withStateLock false
                val requested = pendingNavigation?.takeIf { it.key == owner.key }
                pendingNavigation = null
                if (boundSessionKey != null || requested != null) restored = null
                boundSessionKey = owner.key
                restoreResolvedFor = requested?.key
                backStack.clear()
                mutableState.value =
                    DestinationState(requested?.route ?: AppRoute.Connect, mutableState.value.token + 1)
                true
            }
        } == true
    }

    /** The caller's main scope owns this registration and its SavedStateHandle. */
    fun bindSavedState(
        handle: SavedStateHandle,
        scope: CoroutineScope,
    ): Closeable {
        session().withStateLock {
            if (!savedStateRead) {
                savedStateRead = true
                val profile = handle.get<String>(PROFILE)
                val route = decode(handle.get<String>(KIND), handle.get<String>(CONVERSATION))
                if (profile != null && route != null) restored = SavedRoute(profile, route)
            }
        }
        val job =
            scope.launch {
                state.collect { destination ->
                    save(handle, destination)
                }
            }
        return Closeable { job.cancel() }
    }

    suspend fun restore(
        owner: ActiveSession,
        canOpen: suspend (AppRoute) -> Boolean,
    ): AppRoute? {
        sessionChanged(owner)
        val (candidate, token) =
            session().withStateLock {
                if (restoreResolvedFor == owner.key) return@withStateLock null
                restored to mutableState.value.token
            } ?: return null
        val target =
            if (candidate?.profileId == owner.key.profileId && canOpen(candidate.route)) {
                candidate.route
            } else {
                AppRoute.Chats
            }
        var applied: AppRoute? = null
        admit(DestinationOrigin(owner.key, token)) {
            if (restored != candidate) return@admit
            restored = null
            restoreResolvedFor = owner.key
            backStack.clear()
            mutableState.value = DestinationState(target, token + 1)
            applied = target
        }
        return applied
    }

    private fun save(
        handle: SavedStateHandle,
        destination: DestinationState,
    ) {
        val key = session().withStateLock { boundSessionKey }

        fun writeIfCurrent() {
            session().withStateLock {
                if (restored != null || boundSessionKey != key ||
                    mutableState.value != destination
                ) {
                    return@withStateLock
                }
                handle[PROFILE] = key?.profileId
                handle[KIND] = kind(destination.route)
                handle[CONVERSATION] = conversationId(destination.route)
            }
        }
        // Saved state and route changes share the session admission lock.
        if (key == null) writeIfCurrent() else session().withCurrent(key) { writeIfCurrent() }
    }

    private data class SavedRoute(
        val profileId: String,
        val route: AppRoute,
    )

    private data class PendingNavigation(
        val key: SessionKey,
        val route: AppRoute,
    )

    private fun parent(route: AppRoute): AppRoute =
        when (route) {
            is AppRoute.ConversationDetails -> AppRoute.Chat(route.conversationId)
            is AppRoute.GroupFields -> AppRoute.ConversationDetails(route.conversationId)
            is AppRoute.OwnFields -> AppRoute.ConversationDetails(route.conversationId)
            else -> AppRoute.Chats
        }

    private fun kind(route: AppRoute): String =
        when (route) {
            AppRoute.Connect -> "connect"
            AppRoute.Chats -> "chats"
            AppRoute.Requests -> "requests"
            AppRoute.CreateConversation -> "create"
            is AppRoute.Chat -> "chat"
            is AppRoute.ConversationDetails -> "details"
            is AppRoute.GroupFields -> "group-fields"
            is AppRoute.OwnFields -> "own-fields"
            AppRoute.Settings -> "settings"
            AppRoute.AttachmentRecovery -> "attachments"
        }

    private fun decode(
        kind: String?,
        id: String?,
    ): AppRoute? =
        when (kind) {
            "chats" -> AppRoute.Chats
            "requests" -> AppRoute.Requests
            "create" -> AppRoute.CreateConversation
            "chat" -> id?.let { AppRoute.Chat(it) }
            "details" -> id?.let { AppRoute.ConversationDetails(it) }
            "group-fields" -> id?.let { AppRoute.GroupFields(it) }
            "own-fields" -> id?.let { AppRoute.OwnFields(it) }
            "settings" -> AppRoute.Settings
            "attachments" -> AppRoute.AttachmentRecovery
            else -> null
        }

    companion object {
        private const val PROFILE = "messenger.route.profile"
        private const val KIND = "messenger.route.kind"
        private const val CONVERSATION = "messenger.route.conversation"

        fun conversationId(route: AppRoute): String? =
            when (route) {
                is AppRoute.Chat -> route.conversationId
                is AppRoute.ConversationDetails -> route.conversationId
                is AppRoute.GroupFields -> route.conversationId
                is AppRoute.OwnFields -> route.conversationId
                else -> null
            }
    }
}
