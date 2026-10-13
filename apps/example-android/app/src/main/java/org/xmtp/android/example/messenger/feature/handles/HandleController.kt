package org.xmtp.android.example.messenger.feature.handles

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.xmtp.android.example.shared.feature.handles.*
import uniffi.xmtp_sdk.XmtpException
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import java.util.concurrent.TimeUnit

/** The own inbox's handles: the current one, then the others, most recently claimed first. */
internal data class OwnHandles(
    val current: String?,
    val others: List<String>,
)

/** What the handle editor needs from one signed-in account. */
internal interface HandleAccount {
    suspend fun handles(refresh: Boolean): OwnHandles

    /** The server's published minimum. The editor uses it only to check typed input. */
    fun minNameLength(): Int

    suspend fun availableHandles(name: String): List<String>

    /** The handles among [handles] that an inbox holds now. */
    suspend fun owned(handles: List<String>): Set<String>

    suspend fun claimHandle(handle: String)

    /** The time of each claim this device made, in milliseconds. */
    fun claimTimes(): Map<String, Long>

    fun recordClaim(
        handle: String,
        atMs: Long,
    )

    /** Leave the editor: onboarding opens Chats, and Change handle returns to Settings. */
    fun finish()
}

/**
 * The handle editor for onboarding and Settings. The server decides every claim: this
 * class checks only the grammar and the published minimum of typed input, and claims
 * only a handle that the user picked.
 */
internal class HandleController(
    private val scope: CoroutineScope,
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
    private val now: () -> Long = System::currentTimeMillis,
    private val day: (Long) -> String = ::shortDay,
) {
    private val ui = MutableStateFlow(HandleState())
    val state: StateFlow<HandleState> = ui
    private val lock = Any()
    private var account: HandleAccount? = null

    // `opened` changes for each open and close. `request` changes for each edit, check and claim.
    private var opened = 0L
    private var request = 0L
    private var load: Job? = null
    private var work: Job? = null

    fun open(
        account: HandleAccount,
        mode: HandleMode,
    ) {
        val minimum =
            try {
                account.minNameLength().takeIf { it in 1..MAX_HANDLE_NAME_LENGTH }
            } catch (error: Exception) {
                null
            } ?: DEFAULT_HANDLE_MIN_NAME_LENGTH
        val token =
            synchronized(lock) {
                load?.cancel()
                work?.cancel()
                this.account = account
                ui.value = HandleState(mode = mode, loading = true, minNameLength = minimum)
                ++opened
            }
        load =
            scope.launch(dispatcher) {
                // Show the handles this device holds at once, then the newest from the network.
                val local = read(account, refresh = false)
                if (local != null) show(account, token, local, loading = true)
                val own = read(account, refresh = true) ?: local
                val applied = show(account, token, own, loading = false)
                // An account that already has a handle needs no onboarding step.
                if (applied && mode == HandleMode.CHOOSE && own?.current != null) account.finish()
            }
    }

    private suspend fun read(
        account: HandleAccount,
        refresh: Boolean,
    ): OwnHandles? =
        try {
            account.handles(refresh)
        } catch (error: Throwable) {
            if (error is CancellationException) throw error
            null
        }

    private fun show(
        account: HandleAccount,
        token: Long,
        own: OwnHandles?,
        loading: Boolean,
    ): Boolean {
        val since = own?.current?.let { claimDay(account, it) }
        return update(token) {
            it.copy(
                loading = loading,
                current = own?.current,
                currentSince = since,
                others = own?.others.orEmpty(),
            )
        }
    }

    fun close() {
        synchronized(lock) {
            load?.cancel()
            work?.cancel()
            account = null
            opened++
            request++
            ui.value = HandleState()
        }
    }

    fun dispatch(action: HandleAction) {
        val account = synchronized(lock) { account } ?: return
        when (action) {
            is HandleAction.ChangeInput -> {
                edit(action.value)
            }

            HandleAction.Check -> {
                check(account)
            }

            HandleAction.ShowMore -> {
                change { state ->
                    val status = state.status as? HandleStatus.Suggestions ?: return@change state
                    state.copy(
                        status =
                            status.copy(
                                shown = (status.shown + HANDLE_SUGGESTION_PAGE).coerceAtMost(status.handles.size),
                            ),
                    )
                }
            }

            is HandleAction.Select -> {
                change { state ->
                    val status = state.status as? HandleStatus.Suggestions ?: return@change state
                    if (state.claiming != null || action.handle !in status.visible) return@change state
                    state.copy(status = status.copy(selected = action.handle))
                }
            }

            HandleAction.Claim -> {
                val state = ui.value
                val pick = state.pick ?: return
                if (!state.canClaim) return
                if (state.mode == HandleMode.CHOOSE) {
                    claim(account, pick)
                } else {
                    change { it.copy(confirm = pick) }
                }
            }

            is HandleAction.SwitchBack -> {
                change { state ->
                    if (state.mode != HandleMode.CHANGE || !state.editable || action.handle !in state.others) {
                        state
                    } else {
                        state.copy(confirm = action.handle)
                    }
                }
            }

            HandleAction.ConfirmClaim -> {
                val pick = ui.value.confirm ?: return
                claim(account, pick)
            }

            HandleAction.CancelClaim -> {
                change { it.copy(confirm = null) }
            }

            HandleAction.Retry -> {
                when (val notice = ui.value.notice) {
                    HandleNotice.CheckFailed -> check(account)
                    is HandleNotice.ClaimFailed -> claim(account, notice.handle)
                    else -> Unit
                }
            }

            HandleAction.Skip -> {
                val state = ui.value
                if (state.mode != HandleMode.CHOOSE || state.claiming != null) return
                // Leaving writes the profile store, so it runs off the main thread.
                scope.launch(dispatcher) { account.finish() }
            }
        }
    }

    private fun edit(value: String) {
        synchronized(lock) {
            val state = ui.value
            if (!state.editable) return
            val input = normalizedHandleInput(value)
            work?.cancel()
            request++
            ui.value =
                state.copy(
                    input = input,
                    problem = handleInputProblem(input, state.minNameLength),
                    status = HandleStatus.Editing,
                    confirm = null,
                    notice = null,
                )
        }
    }

    private fun check(account: HandleAccount) {
        val (token, step, input) =
            synchronized(lock) {
                val state = ui.value
                val ready =
                    state.editable && state.input.isNotEmpty() && state.problem == null &&
                        state.notice !is HandleNotice.NotAllowed &&
                        (state.status == HandleStatus.Editing || state.status is HandleStatus.NoneFree)
                if (!ready) return
                work?.cancel()
                ui.value = state.copy(status = HandleStatus.Checking(state.input), confirm = null, notice = null)
                Triple(opened, ++request, state.input)
            }
        work =
            scope.launch(dispatcher) {
                try {
                    val status = lookup(account, input)
                    update(token, step) { it.copy(status = status) }
                } catch (error: XmtpException.InvalidArgument) {
                    update(token, step) {
                        it.copy(status = HandleStatus.Editing, problem = HandleInputProblem.INVALID_CHARACTERS)
                    }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    update(token, step) { it.copy(status = HandleStatus.Editing, notice = HandleNotice.CheckFailed) }
                }
            }
    }

    /** The bare name or the typed handle if it is free, else the free handles for its name part. */
    private suspend fun lookup(
        account: HandleAccount,
        input: String,
    ): HandleStatus {
        val free = account.availableHandles(input)
        if (input in free) return HandleStatus.Available(input)
        val name = handleName(input)
        val handles = (if (name == input) free else account.availableHandles(name)).filter { it != input }
        return suggestions(input, handles, handles.firstOrNull())
    }

    private fun suggestions(
        input: String,
        handles: List<String>,
        selected: String?,
        shown: Int = 0,
    ): HandleStatus {
        if (handles.isEmpty() || selected == null) return HandleStatus.NoneFree(input)
        val visible = maxOf(HANDLE_SUGGESTION_PAGE, shown, handles.indexOf(selected) + 1)
        return HandleStatus.Suggestions(input, handles, visible.coerceAtMost(handles.size), selected)
    }

    private fun claim(
        account: HandleAccount,
        handle: String,
    ) {
        val (token, step, previous) =
            synchronized(lock) {
                val state = ui.value
                if (state.claiming != null || state.blocked) return
                work?.cancel()
                ui.value = state.copy(claiming = handle, confirm = null, notice = null)
                Triple(opened, ++request, state)
            }
        work =
            scope.launch(dispatcher) {
                try {
                    account.claimHandle(handle)
                    claimed(account, token, step, handle)
                } catch (error: XmtpException.HandleTaken) {
                    taken(account, token, step, handle, previous.status)
                } catch (error: XmtpException.HandleNotAllowed) {
                    update(token, step) {
                        if (handle in previous.others) {
                            val short = handleName(handle).length < it.minNameLength
                            it.copy(claiming = null, notice = HandleNotice.SwitchBackBlocked(handle, short))
                        } else {
                            it.copy(
                                claiming = null,
                                status = HandleStatus.Editing,
                                notice = HandleNotice.NotAllowed(handleName(handle)),
                            )
                        }
                    }
                } catch (error: XmtpException.HandleTooSoon) {
                    val notice = tooSoon(account)
                    update(token, step) { blocked(it, notice) }
                } catch (error: XmtpException.HandleLimitReached) {
                    update(token, step) { blocked(it, HandleNotice.LimitReached) }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    // The claim can land before the failure. A second claim would then fail as too soon.
                    val own =
                        try {
                            account.handles(refresh = true)
                        } catch (read: Throwable) {
                            if (read is CancellationException) throw read
                            null
                        }
                    if (own?.current == handle) {
                        claimed(account, token, step, handle)
                    } else {
                        update(token, step) { it.copy(claiming = null, notice = HandleNotice.ClaimFailed(handle)) }
                    }
                }
            }
    }

    private fun claimed(
        account: HandleAccount,
        token: Long,
        step: Long,
        handle: String,
    ) {
        val at = now()
        try {
            account.recordClaim(handle, at)
        } catch (error: Exception) {
            // The claim is in the inbox's log. Only the "since" date is lost.
        }
        val applied =
            update(token, step) { state ->
                state.copy(
                    claiming = null,
                    current = handle,
                    currentSince = day(at),
                    others = (listOfNotNull(state.current) + state.others).filter { it != handle },
                    input = "",
                    problem = null,
                    status = HandleStatus.Editing,
                    notice = null,
                )
            }
        if (applied) account.finish()
    }

    /** Another inbox claimed [handle] first. Show the refreshed suggestions and wait for the user. */
    private suspend fun taken(
        account: HandleAccount,
        token: Long,
        step: Long,
        handle: String,
        previous: HandleStatus,
    ) {
        val input =
            when (previous) {
                is HandleStatus.Suggestions -> previous.input
                is HandleStatus.Available -> previous.handle
                else -> handle
            }
        val earlier = (previous as? HandleStatus.Suggestions)?.handles.orEmpty()
        val candidates = earlier.filter { it != handle }
        val still =
            if (candidates.isEmpty()) {
                candidates
            } else {
                try {
                    val owned = account.owned(candidates)
                    candidates.filter { it !in owned }
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    candidates
                }
            }
        val fresh =
            if (still.size >= HANDLE_SUGGESTION_PAGE) {
                emptyList()
            } else {
                try {
                    account.availableHandles(handleName(input))
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    emptyList()
                }
            }
        val handles = (still + fresh).filter { it != handle }.distinct()
        // The next one is the first remaining suggestion after the one that was taken.
        val position = earlier.indexOf(handle)
        val next = handles.getOrNull(if (position < 0) 0 else earlier.take(position).count { it in still })
        val shown = (previous as? HandleStatus.Suggestions)?.shown ?: 0
        update(token, step) {
            it.copy(
                claiming = null,
                status = suggestions(input, handles, next ?: handles.firstOrNull(), shown),
                notice = HandleNotice.Taken(handle),
            )
        }
    }

    private fun blocked(
        state: HandleState,
        notice: HandleNotice,
    ) = state.copy(
        claiming = null,
        input = "",
        problem = null,
        status = HandleStatus.Editing,
        notice = notice,
    )

    /** The dates come from this device's own claims. A claim from another device has no date here. */
    private fun tooSoon(account: HandleAccount): HandleNotice.TooSoon {
        val last =
            try {
                account.claimTimes().values.maxOrNull()
            } catch (error: Exception) {
                null
            }
        if (last == null || now() - last >= CLAIM_INTERVAL_MS) return HandleNotice.TooSoon(null, null)
        return HandleNotice.TooSoon(day(last), day(last + CLAIM_INTERVAL_MS))
    }

    private fun claimDay(
        account: HandleAccount,
        handle: String,
    ): String? =
        try {
            account.claimTimes()[handle]?.let(day)
        } catch (error: Exception) {
            null
        }

    private fun change(transform: (HandleState) -> HandleState) {
        synchronized(lock) { ui.value = transform(ui.value) }
    }

    private fun update(
        token: Long,
        step: Long? = null,
        transform: (HandleState) -> HandleState,
    ): Boolean =
        synchronized(lock) {
            if (token != opened || (step != null && step != request)) return@synchronized false
            ui.value = transform(ui.value)
            true
        }

    companion object {
        /** The server's fixed interval between claims. It only dates the too-soon message. */
        val CLAIM_INTERVAL_MS = TimeUnit.DAYS.toMillis(7)
    }
}

internal fun shortDay(ms: Long): String =
    DateTimeFormatter
        .ofPattern("MMM d", Locale.getDefault())
        .format(Instant.ofEpochMilli(ms).atZone(ZoneId.systemDefault()))
