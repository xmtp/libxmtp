package org.xmtp.android.library

import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import uniffi.xmtpv3.FfiCatchUpSummary
import uniffi.xmtpv3.resumeStreams
import uniffi.xmtpv3.suspendStreams

/**
 * Counts of what a [Client.catchUpToLive] run brought into the local store, plus
 * whether it reached the live edge.
 */
data class CatchUpSummary(
    val messages: Long,
    val conversations: Long,
    val completed: Boolean,
) {
    internal constructor(ffi: FfiCatchUpSummary) : this(
        messages = ffi.messages.toLong(),
        conversations = ffi.conversations.toLong(),
        completed = ffi.completed,
    )
}

/**
 * Keeps the process-shared streaming wire in step with app foreground/background.
 *
 * The streaming transport is shared across every [Client] in the process, so
 * suspend/resume are process-global, not per-client. This is registered once per
 * process via [ProcessLifecycleOwner] and never torn down — it lives for the
 * process and holds no client state, which is why there's no deregistration hook
 * to get wrong.
 *
 * Two design points, because both are easy to get wrong:
 *
 * - **Activities are already aggregated.** [ProcessLifecycleOwner] emits `ON_START`
 *   when the *first* activity starts and `ON_STOP` when the *last* one stops, so
 *   there is no per-activity counting to do here — androidx has done it.
 *
 * - **Transitions are reconciled, not fired.** Each callback only records the
 *   desired state; a single serial loop drives the wire toward it one op at a
 *   time, re-checking after each. `resumeStreams()` is unbounded while offline,
 *   so a naive launch-per-callback could complete out of order and strand the
 *   wire live-while-backgrounded; the loop instead converges to the last intent.
 *
 * **Known limitation — background launch.** Registration seeds the desired state
 * from [ProcessLifecycleOwner]'s current state, so a process created in the
 * background starts suspended. But a stream opened *after* that while still
 * backgrounded opens a fresh live wire the model can't see (no lifecycle
 * transition fires), so it can sit live-while-backgrounded until the next
 * foreground/background cycle. Backgrounds that only catch up — the normal
 * pattern — are unaffected, since [Client.catchUpToLive] uses its own
 * connection, not this wire. The complete fix is for the shared transport to
 * honor a suspend requested before it is first opened (a libxmtp follow-on).
 */
internal object StreamLifecycleManager {
    private val lock = Any()
    private var registered = false

    // What app lifecycle wants the wire to be. Starts foreground.
    private var desiredLive = true

    // What we last drove the wire to. Streams open live at client creation.
    private var appliedLive = true

    // Whether a reconciler coroutine is currently draining toward desiredLive.
    private var reconciling = false

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    fun enableIfNeeded() {
        synchronized(lock) {
            if (registered) return
            registered = true
        }
        // ProcessLifecycleOwner.addObserver must be called on the main thread;
        // client creation runs on Dispatchers.IO, so hop.
        Handler(Looper.getMainLooper()).post {
            val lifecycle = ProcessLifecycleOwner.get().lifecycle
            lifecycle.addObserver(
                object : DefaultLifecycleObserver {
                    override fun onStart(owner: LifecycleOwner) = setDesired(true)

                    override fun onStop(owner: LifecycleOwner) = setDesired(false)
                },
            )
            // A process launched straight into the background (a service or
            // receiver start, a WorkManager job) sits at CREATED and gets no
            // onStop, so seed the backgrounded case explicitly — foreground is
            // already the default. (Streams opened *after* this while still
            // backgrounded aren't covered; see the class doc.)
            if (!lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) {
                setDesired(false)
            }
        }
    }

    private fun setDesired(live: Boolean) {
        val shouldStart: Boolean
        synchronized(lock) {
            desiredLive = live
            shouldStart = !reconciling && appliedLive != desiredLive
            if (shouldStart) reconciling = true
        }
        if (shouldStart) scope.launch { reconcile() }
    }

    /**
     * Drives the wire toward [desiredLive], one op at a time, until they agree.
     * The lock is only held for the synchronous state reads/writes, never across
     * the suspending op.
     *
     * A failed op does *not* advance [appliedLive]: recording a transition that
     * never happened would leave the wire stuck in the wrong state with no retry.
     * Instead the reconciler stops and leaves [appliedLive] misaligned, so the
     * next foreground/background transition re-runs the op it was owed.
     */
    private suspend fun reconcile() {
        while (true) {
            val target =
                synchronized(lock) {
                    if (desiredLive == appliedLive) {
                        reconciling = false
                        return
                    }
                    desiredLive
                }
            val result =
                runCatching {
                    if (target) resumeStreams() else suspendStreams()
                }
            if (result.isFailure) {
                Log.w(
                    "XMTP",
                    "Stream ${if (target) "resume" else "suspend"} failed; " +
                        "retrying on the next lifecycle transition",
                    result.exceptionOrNull(),
                )
                synchronized(lock) { reconciling = false }
                return
            }
            synchronized(lock) { appliedLive = target }
        }
    }
}
