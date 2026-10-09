package org.xmtp.android.example.messenger.notifications

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.messenger.AppSession
import uniffi.xmtp_sdk.*

class NotificationRestoreRaceInstrumentedTest {
    @Test fun concurrentForegroundAndPushRestoreKeepOneOwnerWithDefaultReaders() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val context = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
            val original = AppSession(context)
            val restored = AppSession(context)
            val lookupEntered = CompletableDeferred<Unit>()
            val releaseLookup = CompletableDeferred<Unit>()
            val pushOpened = CompletableDeferred<Unit>()
            var foreground: Deferred<Unit>? = null
            var push: Deferred<org.xmtp.android.example.messenger.ActiveSession?>? = null
            try {
                original.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val inbox = checkNotNull(original.active.value).client.inboxId()
                original.signOut()
                original.preferences.setSignedIn(true)
                restored.beforeAutomaticProfileLookup = {
                    lookupEntered.complete(Unit)
                    releaseLookup.await()
                }
                restored.beforeOpeningListener = {
                    if (currentCoroutineContext()[CoroutineName]?.name == "push-restore") pushOpened.complete(Unit)
                }
                foreground = async(Dispatchers.IO + CoroutineName("foreground-restore")) { restored.restore() }
                withTimeout(30_000) { lookupEntered.await() }
                push =
                    async(Dispatchers.IO + CoroutineName("push-restore"), start = CoroutineStart.UNDISPATCHED) {
                        restored.restoreForPush()
                    }
                releaseLookup.complete(Unit)
                withTimeout(30_000) { foreground.await() }
                val fromPush = withTimeout(30_000) { checkNotNull(push.await()) }
                assertFalse("A push must not displace foreground restore with a cold owner", pushOpened.isCompleted)
                val owner = checkNotNull(restored.active.value)
                assertSame(owner, fromPush)
                assertEquals(inbox, owner.client.inboxId())
                withTimeout(30_000) { restored.connection.first { it.contains("Connected") } }
                val error =
                    withTimeout(30_000) {
                        runCatching {
                            owner.client.conversations
                                .streamAllMessages()
                                .collect { }
                        }.exceptionOrNull()
                    }
                assertTrue("Foreground restore must own the default reader", error is XmtpException.ConsumerOwned)
                println("PUSH_RESTORE_RACE same_owner=true foreground_reader_owned=true push_opened=false")
            } finally {
                releaseLookup.complete(Unit)
                withContext(NonCancellable) {
                    foreground?.cancelAndJoin()
                    push?.cancelAndJoin()
                    original.signOut()
                    restored.beforeAutomaticProfileLookup = {}
                    restored.beforeOpeningListener = {}
                    if (restored.preferences.active() != null ||
                        restored.preferences.reset() != null
                    ) {
                        restored.deleteAccount()
                    }
                    restored.signOut()
                    AndroidStreamLifecycle.enabled = true
                }
            }
        }
}
