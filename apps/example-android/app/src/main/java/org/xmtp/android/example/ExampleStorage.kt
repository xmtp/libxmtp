package org.xmtp.android.example

import uniffi.xmtp_sdk.InboxId
import uniffi.xmtp_sdk.StorageLocation
import java.io.File
import java.nio.file.Files
import java.nio.file.NoSuchFileException

private val legacyDatabaseName = Regex("xmtp-local-[0-9a-f]{64}\\.db3")

/** Reopen this account's old file without moving or selecting another account's data. */
internal suspend fun exampleStorageLocation(
    filesDir: File,
    inboxIdFor: suspend () -> InboxId,
): StorageLocation {
    val directory = File(filesDir, "xmtp_db")
    val legacyDatabases =
        try {
            Files.newDirectoryStream(directory.toPath()).use { paths ->
                paths.map { it.toFile() }.filter { it.isFile && legacyDatabaseName.matches(it.name) }
            }
        } catch (_: NoSuchFileException) {
            return StorageLocation.Default
        }
    if (legacyDatabases.isEmpty()) return StorageLocation.Default

    val inboxId = inboxIdFor()
    val database =
        legacyDatabases.firstOrNull { it.name == "xmtp-local-$inboxId.db3" }
            ?: return StorageLocation.Default

    return StorageLocation.Explicit(
        dbPath = database.absolutePath,
        attachmentsDir = File(directory, "xmtp-local-$inboxId-attachments").absolutePath,
    )
}
