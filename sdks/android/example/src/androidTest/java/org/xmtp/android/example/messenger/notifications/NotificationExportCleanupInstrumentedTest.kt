package org.xmtp.android.example.messenger.notifications

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.ExampleApp
import org.xmtp.android.example.messenger.AppSession
import org.xmtp.android.example.messenger.BackendProfile
import org.xmtp.android.example.messenger.attachments.AttachmentFiles
import java.io.File
import java.util.UUID

class NotificationExportCleanupInstrumentedTest {
    @Test fun signedOutColdPushAwaitsExportCleanupBeforeItsAdmissionReturn() = cleanupBeforeAdmission(receive = false)

    @Test fun unconfiguredReceiveAwaitsExportCleanupBeforeItsAdmissionReturn() = cleanupBeforeAdmission(receive = true)

    private fun cleanupBeforeAdmission(receive: Boolean) =
        runBlocking<Unit> {
            withTimeout(30_000) {
                val context = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
                (context as ExampleApp).session.awaitStartupExportCleanup()
                val session = AppSession(context)
                // The later journal must be handled by the awaited entry, not constructor startup.
                session.awaitStartupExportCleanup()
                val profile = BackendProfile(UUID.randomUUID().toString(), "https://example.invalid")
                val directory = AttachmentFiles.profileDirectory(context, profile.id)
                val database = profile.paths(context.filesDir).database
                var builds = 0
                var controller: NotificationController? = null
                session.beforeClientBuild = { builds += 1 }
                try {
                    session.preferences.setActive(profile)
                    check(directory.mkdirs())
                    File(directory, "recorded-export").writeText("local export cleanup")
                    session.preferences.beginExportCleanup(profile.id)
                    assertFalse(session.preferences.signedIn())
                    assertEquals(setOf(profile.id), session.preferences.pendingExportCleanup())
                    assertTrue(directory.exists())
                    assertFalse(database.exists())
                    if (receive) {
                        val transport =
                            object : PushTransport {
                                override val configured = false

                                override fun requestToken(callback: (String?, Throwable?) -> Unit) =
                                    error("Off must not request a token")
                            }
                        val receiving = NotificationController(context, session, transport)
                        controller = receiving
                        assertFalse(receiving.configured)
                        receiving.permissionGranted = {
                            error("Unconfigured receive must return before checking permission")
                        }
                        assertFalse(receiving.receive(emptyMap()))
                    } else {
                        assertNull(session.restoreForPush())
                    }
                    assertFalse(
                        "The awaited cold entry removes the recorded export before it returns",
                        directory.exists(),
                    )
                    assertTrue(
                        "The completed local replay clears its durable journal",
                        session.preferences.pendingExportCleanup().isEmpty(),
                    )
                    assertFalse(session.preferences.signedIn())
                    assertNull(session.active.value)
                    assertEquals(
                        "Local replay and rejected push must not build an SDK client",
                        0,
                        builds,
                    )
                    assertFalse(
                        "Local replay must not create a missing database",
                        database.exists(),
                    )
                    println(
                        "PUSH_EXPORT_REPLAY receive=$receive signed_out=true " +
                            "sdk_builds=$builds journal_empty=true",
                    )
                } finally {
                    withContext(NonCancellable) {
                        controller?.close()
                        check(!directory.exists() || directory.deleteRecursively())
                        session.preferences.completeExportCleanup(profile.id)
                        session.preferences.removeProfile(profile.id)
                        session.beforeClientBuild = {}
                    }
                }
            }
        }
}
