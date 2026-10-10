package org.xmtp.android.example.messenger.attachments

import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.core.stringSetPreferencesKey
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.messenger.MessengerPreferences
import java.io.File
import java.nio.file.Files

class ExportCleanupJournalTest {
    @Test fun signedOutStateAndCleanupReferenceShareOneDurableSnapshot() =
        runBlocking<Unit> {
            val directory = Files.createTempDirectory("export-journal-").toFile()
            val file = File(directory, "messenger.preferences_pb")
            val firstScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
            val secondScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
            try {
                val firstStore = PreferenceDataStoreFactory.create(scope = firstScope, produceFile = { file })
                val preferences = MessengerPreferences(firstStore)
                preferences.setSignedIn(true)
                val observed =
                    async {
                        firstStore.data.first { it[stringPreferencesKey("signed-in")] == "false" }
                    }
                preferences.beginExportCleanup("profile")
                val snapshot = observed.await()
                assertEquals(setOf("profile"), snapshot[stringSetPreferencesKey("pending-export-cleanup")])
                firstScope.coroutineContext[Job]!!.cancelAndJoin()
                val coldStore = PreferenceDataStoreFactory.create(scope = secondScope, produceFile = { file })
                val reconstructed = MessengerPreferences(coldStore)
                assertFalse(reconstructed.signedIn())
                assertEquals(
                    "A cold store retains the sign-out cleanup obligation",
                    setOf("profile"),
                    reconstructed.pendingExportCleanup(),
                )
            } finally {
                firstScope.coroutineContext[Job]!!.cancelAndJoin()
                secondScope.coroutineContext[Job]!!.cancelAndJoin()
                directory.deleteRecursively()
            }
        }

    @Test fun failedCleanupRetainsItsDurableReferenceForTheNextReplay() =
        runBlocking<Unit> {
            val directory = Files.createTempDirectory("export-replay-").toFile()
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
            try {
                val store =
                    PreferenceDataStoreFactory.create(scope = scope, produceFile = {
                        File(directory, "messenger.preferences_pb")
                    })
                val preferences = MessengerPreferences(store)
                preferences.beginExportCleanup("profile")
                val calls = mutableListOf<String>()
                val failed =
                    ExportCleanupReplay(preferences::pendingExportCleanup, {
                        error("Export deletion failed")
                    }, preferences::completeExportCleanup)
                assertEquals("Export deletion failed", runCatching { failed.run() }.exceptionOrNull()?.message)
                assertEquals(setOf("profile"), preferences.pendingExportCleanup())
                val replay =
                    ExportCleanupReplay(
                        preferences::pendingExportCleanup,
                        { profile -> calls += "revoke-delete:$profile" },
                        { profile ->
                            calls += "complete:$profile"
                            preferences.completeExportCleanup(profile)
                        },
                    )
                replay.run()
                assertEquals(listOf("revoke-delete:profile", "complete:profile"), calls)
                assertTrue(preferences.pendingExportCleanup().isEmpty())
                replay.run()
                assertEquals(2, calls.size)
            } finally {
                scope.coroutineContext[Job]!!.cancelAndJoin()
                directory.deleteRecursively()
            }
        }

    @Test fun replayCompletesOnlyItsOwnProfileAndKeepsOtherPendingProfiles() =
        runBlocking<Unit> {
            val directory = Files.createTempDirectory("export-profiles-").toFile()
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
            try {
                val store =
                    PreferenceDataStoreFactory.create(scope = scope, produceFile = {
                        File(directory, "messenger.preferences_pb")
                    })
                val preferences = MessengerPreferences(store)
                preferences.beginExportCleanup("one")
                preferences.beginExportCleanup("two")
                preferences.completeExportCleanup("one")
                assertEquals(setOf("two"), preferences.pendingExportCleanup())
            } finally {
                scope.coroutineContext[Job]!!.cancelAndJoin()
                directory.deleteRecursively()
            }
        }
}
