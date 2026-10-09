package org.xmtp.android.example.messenger

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.example.BuildConfig
import uniffi.xmtp_sdk.AndroidStreamLifecycle

@RunWith(AndroidJUnit4::class)
class SessionLifecycleInstrumentedTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
    @Test fun persistedSignerSignOutAndColdResetKeepProfileBoundaries() = runBlocking {
        AndroidStreamLifecycle.enabled = false
        val session = AppSession(context)
        try {
            session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
            val first = checkNotNull(session.active.value)
            val wallet = checkNotNull(session.secrets.read(first.key.profileId, "wallet"))
            val inbox = first.client.inboxId()
            session.preferences.saveDraft(first.key.profileId, SendDraftRef("unknown-send", "chat", phase = SendPhase.QUEUEING))
            val unrelated = File(context.filesDir, "unrelated-reset-proof").apply { writeText("retained") }
            val staged = File(first.paths.temp, "staged").apply { parentFile.mkdirs(); writeText("plaintext") }
            val downloaded = File(first.paths.attachments, "downloaded").apply { writeText("plaintext") }
            val exported = File(first.paths.exports, "exported").apply { parentFile.mkdirs(); writeText("plaintext") }
            session.unregisterNotifications = { error("Unregister network failure") }
            session.signOut()
            assertFalse(session.preferences.signedIn())
            assertNull(session.active.value)
            assertFalse(session.accepts(first.key))
            assertTrue(first.paths.database.exists())
            assertNull(session.secrets.read(first.key.profileId, "credential"))
            assertArrayEquals(wallet, session.secrets.read(first.key.profileId, "wallet"))
            session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
            assertEquals(inbox, checkNotNull(session.active.value).client.inboxId())
            assertEquals(1, session.preferences.drafts(first.key.profileId).count { it.phase == SendPhase.QUEUEING && it.acceptedMessageId == null })
            // Simulate process death in STOPPING before native deletion. No handle is restored.
            session.signOut()
            session.preferences.saveReset(first.paths.resetRecord(first.key.profileId))
            val recreated = AppSession(context)
            recreated.restore()
            assertNull(recreated.active.value)
            assertNull(recreated.preferences.reset())
            assertFalse(first.paths.database.exists())
            assertFalse(staged.exists()); assertFalse(downloaded.exists()); assertFalse(exported.exists())
            assertTrue(unrelated.exists()); unrelated.delete()
            assertFalse(recreated.preferences.profiles().any { it.id == first.key.profileId })
        } finally { withContext(NonCancellable) { session.signOut() }; AndroidStreamLifecycle.enabled = true }
    }
}
