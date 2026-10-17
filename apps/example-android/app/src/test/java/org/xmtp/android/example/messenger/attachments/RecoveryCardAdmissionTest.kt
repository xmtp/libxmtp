package org.xmtp.android.example.messenger.attachments

import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.messenger.MessengerPreferences
import org.xmtp.android.example.messenger.SendDraftRef
import org.xmtp.android.example.messenger.SendPhase
import java.io.File
import java.nio.file.Files

class RecoveryCardAdmissionTest {
    private fun withPreferences(check: suspend (MessengerPreferences) -> Unit) =
        runBlocking<Unit> {
            val directory = Files.createTempDirectory("recovery-card-").toFile()
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
            try {
                val store =
                    PreferenceDataStoreFactory.create(scope = scope, produceFile = {
                        File(directory, "messenger.preferences_pb")
                    })
                check(MessengerPreferences(store))
            } finally {
                scope.coroutineContext[Job]!!.cancelAndJoin()
                directory.deleteRecursively()
            }
        }

    @Test fun aRemovedSnapshotCannotRestoreItsCard() =
        withPreferences { preferences ->
            val snapshot = SendDraftRef("draft", "chat", "secret", "accepted", SendPhase.ACCEPTED)
            preferences.saveDraft("profile", snapshot)
            val old = preferences.drafts("profile").single()
            preferences.removeDraft("profile", snapshot.draftId)
            var emitted = false
            preferences.admitDraftSnapshot("profile", old) { emitted = true }
            assertFalse("A published reference cannot regain a recovery action", emitted)
            assertTrue(preferences.drafts("profile").isEmpty())
        }

    @Test fun aChangedOwnerOrPhaseCannotEmitAnOldCard() =
        withPreferences { preferences ->
            val snapshot = SendDraftRef("draft", "chat", "secret")
            preferences.saveDraft("profile", snapshot)
            val latest = snapshot.copy(phase = SendPhase.ACCEPTED, acceptedMessageId = "accepted")
            preferences.saveDraft("profile", latest)
            var emitted = false
            preferences.admitDraftSnapshot("profile", snapshot) { emitted = true }
            assertFalse("The old phase cannot replace the accepted recovery action", emitted)
            assertEquals(listOf(latest), preferences.drafts("profile"))
        }

    @Test fun theCurrentSnapshotCanEmitWithoutChangingOtherDrafts() =
        withPreferences { preferences ->
            val snapshot = SendDraftRef("draft", "chat", "secret")
            val other = SendDraftRef("other", "other-chat", "other-secret")
            preferences.saveDraft("profile", snapshot)
            preferences.saveDraft("profile", other)
            var emitted = 0
            preferences.admitDraftSnapshot("profile", snapshot) { emitted++ }
            assertEquals(1, emitted)
            assertEquals(listOf(snapshot, other), preferences.drafts("profile"))
        }
}
