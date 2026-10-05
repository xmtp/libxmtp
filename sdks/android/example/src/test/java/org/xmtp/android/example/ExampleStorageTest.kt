package org.xmtp.android.example

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.xmtp_sdk.StorageLocation
import java.io.File
import java.nio.file.NotDirectoryException

class ExampleStorageTest {
    @get:Rule
    val temporary = TemporaryFolder()

    private val inboxId = "ab".repeat(32)
    private val otherInboxId = "cd".repeat(32)

    private fun legacyDatabase(id: String): File =
        File(temporary.root, "xmtp_db/xmtp-local-$id.db3").apply {
            parentFile.mkdirs()
            writeText("stored identity $id")
        }

    @Test
    fun existingAccountKeepsItsDatabasePath() =
        runBlocking {
            val database = legacyDatabase(inboxId)
            val otherDatabase = legacyDatabase(otherInboxId)
            val location = exampleStorageLocation(temporary.root) { inboxId }

            assertEquals(
                StorageLocation.Explicit(
                    database.absolutePath,
                    File(database.parentFile, "xmtp-local-$inboxId-attachments").absolutePath,
                ),
                location,
            )
            assertEquals("stored identity $inboxId", database.readText())
            assertEquals("stored identity $otherInboxId", otherDatabase.readText())
        }

    @Test
    fun anotherAccountsFileDoesNotReplaceTheDefaultLocation() =
        runBlocking {
            legacyDatabase(otherInboxId)
            assertEquals(StorageLocation.Default, exampleStorageLocation(temporary.root) { inboxId })
        }

    @Test
    fun freshInstallKeepsDefaultWithoutLookup() =
        runBlocking {
            val location = exampleStorageLocation(temporary.root) { error("Unexpected lookup") }
            assertEquals(StorageLocation.Default, location)
        }

    @Test
    fun emptyDirectoryKeepsDefaultWithoutLookup() =
        runBlocking {
            File(temporary.root, "xmtp_db").mkdir()
            val location = exampleStorageLocation(temporary.root) { error("Unexpected lookup") }
            assertEquals(StorageLocation.Default, location)
        }

    @Test
    fun directoryReadFailureDoesNotFallBackToDefault() =
        runBlocking {
            val blockedDirectory = File(temporary.root, "xmtp_db").apply { writeText("not a directory") }
            try {
                exampleStorageLocation(temporary.root) { error("Unexpected lookup") }
                fail("Directory read failure must propagate")
            } catch (error: NotDirectoryException) {
                assertEquals(blockedDirectory.absolutePath, error.file)
            }
            assertEquals("not a directory", blockedDirectory.readText())
        }

    @Test
    fun lookupFailureDoesNotSelectAFileOrFallBackToDefault() =
        runBlocking {
            val database = legacyDatabase(inboxId)
            val failure = IllegalStateException("Backend unavailable")
            try {
                exampleStorageLocation(temporary.root) { throw failure }
                fail("Lookup failure must propagate")
            } catch (error: IllegalStateException) {
                assertSame(failure, error)
            }
            assertEquals("stored identity $inboxId", database.readText())
        }
}
