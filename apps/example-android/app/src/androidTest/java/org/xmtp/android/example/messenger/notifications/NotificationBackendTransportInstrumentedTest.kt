package org.xmtp.android.example.messenger.notifications

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.messenger.*
import uniffi.xmtp_sdk.*

class NotificationBackendTransportInstrumentedTest {
    @Test fun savedRemoteHttpPushRestoreRejectsBeforeCredentialsOrSdkConstruction() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val context = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
            val original = AppSession(context)
            val recreated = AppSession(context)
            var sdkStarts = 0
            try {
                original.connect(BuildConfig.XMTP_BACKEND_URL, "", localAttachmentNetwork(BuildConfig.XMTP_BACKEND_URL))
                val initial = checkNotNull(original.active.value)
                val profile = initial.profile
                val database = initial.paths.database
                assertTrue(database.isFile)
                original.signOut()
                val saved = profile.copy(backend = "http://saved-push-remote.example.test", allowPrivateNetwork = true)
                original.preferences.setActive(saved)
                original.preferences.setSignedIn(true)
                val credential = "saved-push-test-only-secret".toByteArray()
                original.secrets.write(saved.id, "credential", credential)
                recreated.needsNotificationPreflight = { true }
                recreated.beforeClientBuild = {
                    sdkStarts += 1
                    error("Saved remote HTTP reached the SDK construction boundary")
                }
                val result = runCatching { recreated.restoreForPush() }
                assertTrue(
                    "The saved URL must fail transport validation",
                    result.exceptionOrNull() is IllegalArgumentException,
                )
                assertEquals(0, sdkStarts)
                assertNull(recreated.active.value)
                assertEquals(saved, recreated.preferences.active())
                assertTrue(recreated.preferences.signedIn())
                assertTrue(database.isFile)
                assertArrayEquals(credential, recreated.secrets.read(saved.id, "credential"))
                println("PUSH_TRANSPORT_PROOF saved_complete_profile=true remote_http_rejected=true sdk_starts=0")
            } finally {
                withContext(NonCancellable) {
                    original.signOut()
                    recreated.beforeClientBuild = {}
                    if (recreated.preferences.active() != null ||
                        recreated.preferences.reset() != null
                    ) {
                        recreated.deleteAccount()
                    }
                    recreated.signOut()
                    AndroidStreamLifecycle.enabled = true
                }
            }
        }
}
