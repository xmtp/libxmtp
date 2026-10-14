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
    /** The handles this device knows. It does not ask the network. */
    suspend fun handles(): OwnHandles

    /** The server's published minimum. The editor uses it only to check typed input. */
    fun minNameLength(): Int

    suspend fun availableHandles(name: String): List<String>

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

    // Each open and close cancels both jobs under [lock]; each edit, check, and claim cancels [work].
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
        val job =
            synchronized(lock) {
                load?.cancel()
                work?.cancel()
                this.account = account
                ui.value = HandleState(mode = mode, loading = true, minNameLength = minimum)
                task { readOwn(account, mode, it) }.also { load = it }
            }
        job.start()
    }

    private suspend fun readOwn(
        account: HandleAccount,
        mode: HandleMode,
        job: Job,
    ) {
        val own =
            try {
                account.handles()
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                // The server decides every claim, so unknown handles show as none.
                OwnHandles(null, emptyList())
            }
        val since = own.current?.let { claimDay(account, it) }
        val applied =
            update(job) {
                it.copy(loading = false, current = own.current, currentSince = since, others = own.others)
            }
        // An account that already has a handle needs no onboarding step.
        if (applied && mode == HandleMode.CHOOSE && own.current != null) account.finish()
    }

    fun close() {
        synchronized(lock) {
            load?.cancel()
            work?.cancel()
            account = null
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
                change { switchBack(it, action.handle) ?: it }
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
        val job =
            synchronized(lock) {
                val state = ui.value
                val ready =
                    state.editable && state.input.isNotEmpty() && state.problem == null &&
                        state.notice !is HandleNotice.NotAllowed &&
                        (state.status == HandleStatus.Editing || state.status is HandleStatus.NoneFree)
                if (!ready) return
                // The server lists no handle that this inbox holds as free, so the editor answers for those.
                val own = state.copy(status = HandleStatus.Editing, notice = null)
                if (state.input == state.current) {
                    ui.value = own.copy(problem = HandleInputProblem.CURRENT)
                    return
                }
                switchBack(own, state.input)?.let {
                    ui.value = it
                    return
                }
                work?.cancel()
                ui.value = state.copy(status = HandleStatus.Checking(state.input), confirm = null, notice = null)
                task { answer(account, it, state.input) }.also { work = it }
            }
        job.start()
    }

    /** A switch back to [handle], one of the inbox's other handles, asks for confirmation first (D34). */
    private fun switchBack(
        state: HandleState,
        handle: String,
    ): HandleState? =
        if (state.mode == HandleMode.CHANGE && state.editable && handle in state.others) {
            state.copy(confirm = handle)
        } else {
            null
        }

    /**
     * Show the server's answer for [input], with [notice]. It never offers [except]: after
     * HandleTaken, a lagging replica can still list the taken handle as free.
     */
    private suspend fun answer(
        account: HandleAccount,
        job: Job,
        input: String,
        except: String? = null,
        notice: HandleNotice? = null,
    ) {
        try {
            val status = lookup(account, input, except)
            update(job) { it.copy(claiming = null, status = status, notice = notice) }
        } catch (error: XmtpException.InvalidArgument) {
            update(job) {
                it.copy(claiming = null, status = HandleStatus.Editing, problem = HandleInputProblem.INVALID_CHARACTERS)
            }
        } catch (error: Throwable) {
            if (error is CancellationException) throw error
            update(job) {
                it.copy(claiming = null, status = HandleStatus.Editing, notice = HandleNotice.CheckFailed)
            }
        }
    }

    /** The bare name or the typed handle if it is free, else the free handles for its name part. */
    private suspend fun lookup(
        account: HandleAccount,
        input: String,
        except: String?,
    ): HandleStatus {
        val free = account.availableHandles(input).filter { it != except }
        if (input in free) return HandleStatus.Available(input)
        val name = handleName(input)
        val handles =
            (if (name == input) free else account.availableHandles(name)).filter { it != input && it != except }
        if (handles.isEmpty()) return HandleStatus.NoneFree(input)
        return HandleStatus.Suggestions(input, handles, minOf(HANDLE_SUGGESTION_PAGE, handles.size), handles.first())
    }

    private fun claim(
        account: HandleAccount,
        handle: String,
    ) {
        val job =
            synchronized(lock) {
                val state = ui.value
                if (!state.editable) return
                work?.cancel()
                ui.value = state.copy(claiming = handle, confirm = null, notice = null)
                task { send(account, it, handle, state) }.also { work = it }
            }
        job.start()
    }

    /** Claim [handle] and show the server's answer. [previous] is the state before the claim. */
    private suspend fun send(
        account: HandleAccount,
        job: Job,
        handle: String,
        previous: HandleState,
    ) {
        try {
            account.claimHandle(handle)
            claimed(account, job, handle)
        } catch (error: XmtpException.HandleTaken) {
            // Show fresh suggestions and wait for the user's pick (PD9).
            val input =
                when (val status = previous.status) {
                    is HandleStatus.Suggestions -> status.input
                    is HandleStatus.Available -> status.handle
                    else -> handle
                }
            answer(account, job, input, except = handle, notice = HandleNotice.Taken(handle))
        } catch (error: XmtpException.HandleNotAllowed) {
            update(job) {
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
            update(job) { blocked(it, tooSoon(account)) }
        } catch (error: XmtpException.HandleLimitReached) {
            update(job) { blocked(it, HandleNotice.LimitReached) }
        } catch (error: Throwable) {
            if (error is CancellationException) throw error
            update(job) { it.copy(claiming = null, notice = HandleNotice.ClaimFailed(handle)) }
        }
    }

    private fun claimed(
        account: HandleAccount,
        job: Job,
        handle: String,
    ) {
        val at = now()
        try {
            account.recordClaim(handle, at)
        } catch (error: Exception) {
            // The claim is in the inbox's log. Only the "since" date is lost.
        }
        val applied =
            update(job) { state ->
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

    /**
     * A job for [block] that has not started. The caller stores it under [lock] and then starts
     * it, so a cancel under [lock] always reaches the job before its answer can apply.
     */
    private fun task(block: suspend (Job) -> Unit): Job =
        scope.launch(dispatcher, CoroutineStart.LAZY) { block(coroutineContext.job) }

    /** Apply [transform] unless an open, close, edit, check, or claim has cancelled [job]. */
    private fun update(
        job: Job,
        transform: (HandleState) -> HandleState,
    ): Boolean =
        synchronized(lock) {
            if (!job.isActive) return@synchronized false
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
