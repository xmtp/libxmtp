package org.xmtp.android.example.messenger.attachments

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.*
import uniffi.xmtp_sdk.*
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

class AttachmentLifetimeInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(check: () -> Boolean) = withTimeout(30_000) { while (!check()) delay(20) }

    @Test fun ownerClosureDuringWatcherRecoveryDoesNotEscapeItsCoroutine() = runBlocking<Unit> { lifecycle("recovery") }

    @Test fun staleWatcherInitializationDoesNotReadMissingServices() = runBlocking<Unit> { lifecycle("initialization") }

    private suspend fun lifecycle(stage: String) =
        coroutineScope {
            val enabled = AndroidStreamLifecycle.enabled
            val ready = CompletableDeferred<Unit>()
            val vmRelease = CompletableDeferred<Unit>()
            val entered = CompletableDeferred<Unit>()
            val watcherJob = CompletableDeferred<Job>()
            val finished = CompletableDeferred<Unit>()
            val release = CountDownLatch(1)
            val paused = AtomicBoolean()
            val uncaught = AtomicReference<Throwable?>()
            val previousHandler = Thread.getDefaultUncaughtExceptionHandler()
            val session = model.session
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                session.signOut()
                compose.activityRule.scenario.recreate()
                val host = compose.activity.attachments
                Thread.setDefaultUncaughtExceptionHandler { thread, failure ->
                    if (failure.stackTrace.any { it.className.contains("attachments.AttachmentHost") }) {
                        uncaught.compareAndSet(null, failure)
                    } else {
                        previousHandler?.uncaughtException(thread, failure)
                    }
                }
                model.beforeActiveRefresh = {
                    ready.complete(Unit)
                    vmRelease.await()
                }
                host.beforeWatchRefresh = {
                    ready.await()
                    watcherJob.complete(checkNotNull(currentCoroutineContext()[Job]))
                    if (stage == "initialization" && paused.compareAndSet(false, true)) {
                        entered.complete(Unit)
                        check(release.await(30, TimeUnit.SECONDS))
                    }
                }
                host.beforeWatchRecovery = {
                    if (stage == "recovery" && paused.compareAndSet(false, true)) {
                        entered.complete(Unit)
                        check(release.await(30, TimeUnit.SECONDS))
                    }
                }
                host.onWatchFinished = { finished.complete(Unit) }
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
                val owner = checkNotNull(session.active.value)
                withTimeout(30_000) { entered.await() }
                val ending = async { session.signOut() }
                until { !session.accepts(owner.key) }
                release.countDown()
                vmRelease.complete(Unit)
                withTimeout(30_000) {
                    ending.await()
                    finished.await()
                    watcherJob.await().join()
                }
                assertNull("No uncaught attachment watcher failure at $stage", uncaught.get())
                assertNull(session.active.value)
                host.beforeWatchRefresh = {}
                host.beforeWatchRecovery = {}
                host.onWatchFinished = {}
                model.beforeActiveRefresh = {}
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
                val next = checkNotNull(session.active.value)
                until { model.state.value.inbox == next.client.inboxId() && model.state.value.features.attachments }
                assertTrue(session.accepts(next.key))
                println("ATTACHMENT_LIFETIME_PROOF stage=$stage no-uncaught-failure=true next-owner=true")
            } finally {
                release.countDown()
                vmRelease.complete(Unit)
                Thread.setDefaultUncaughtExceptionHandler(previousHandler)
                model.beforeActiveRefresh = {}
                compose.activity.attachments.beforeWatchRefresh = {}
                compose.activity.attachments.beforeWatchRecovery = {}
                compose.activity.attachments.onWatchFinished = {}
                withContext(NonCancellable) {
                    if (session.active.value != null || session.preferences.reset() != null) session.deleteAccount()
                    session.signOut()
                    AndroidStreamLifecycle.enabled = enabled
                }
            }
        }
}
