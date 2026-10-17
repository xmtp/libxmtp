package org.xmtp.android.example.messenger.attachments

import android.content.ClipData
import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import uniffi.xmtp_sdk.DownloadedAttachment
import java.io.File
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest

/** A prepared file is released unless an external viewer was launched. */
internal class PreparedAttachmentExport(
    val intent: Intent,
    private val finish: (Boolean) -> Unit,
) : AutoCloseable {
    private var finished = false

    @Synchronized fun launched() {
        if (finished) return
        finished = true
        finish(true)
    }

    @Synchronized override fun close() {
        if (finished) return
        finished = true
        finish(false)
    }
}

/** Process ownership for immutable profile exports. SDK files remain owned by the SDK. */
internal object AttachmentExports {
    private data class Entry(
        val file: File,
        var leases: Int = 0,
        var committed: Boolean = false,
    )

    private val preparation = Mutex()
    private val entries = mutableMapOf<String, Entry>()

    suspend fun prepare(
        context: Context,
        profileId: String,
        downloaded: DownloadedAttachment,
        accepts: () -> Boolean,
    ): PreparedAttachmentExport {
        var lease: PreparedAttachmentExport? = null
        try {
            return withContext(Dispatchers.IO) {
                preparation.withLock {
                    check(accepts()) { "The file view changed" }
                    val directory = AttachmentFiles.profileDirectory(context, profileId)
                    check(directory.isDirectory || directory.mkdirs()) { "Cannot create the export directory" }
                    directory.listFiles()?.filter { it.name.startsWith("preparing-") }?.forEach {
                        check(it.delete() || !it.exists()) { "Cannot remove an interrupted export" }
                    }
                    // The SDK path contains the attachment key and safe filename. Its verified bytes are immutable.
                    val digest = MessageDigest.getInstance("SHA-256").digest(downloaded.path.toByteArray())
                    val name = digest.joinToString("") { "%02x".format(it) }
                    val file = File(directory, "file-$name")
                    val entry =
                        synchronized(entries) {
                            // An export from a previous process can still have a live viewer grant.
                            entries
                                .getOrPut(file.absolutePath) { Entry(file, committed = file.exists()) }
                                .also { it.leases++ }
                        }
                    try {
                        if (!file.exists()) {
                            val staging = File.createTempFile("preparing-", ".tmp", directory)
                            try {
                                File(downloaded.path).inputStream().use { input ->
                                    staging.outputStream().use { output ->
                                        PrivateFileStager.copy(input, output, ULong.MAX_VALUE)
                                        output.fd.sync()
                                    }
                                }
                                check(accepts()) { "The file view changed" }
                                Files.move(staging.toPath(), file.toPath(), StandardCopyOption.ATOMIC_MOVE)
                            } finally {
                                staging.delete()
                            }
                        }
                        check(accepts()) { "The file view changed" }
                        val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
                        val intent =
                            Intent(Intent.ACTION_VIEW)
                                .setDataAndType(uri, downloaded.mimeType ?: "application/octet-stream")
                                .apply {
                                    clipData = ClipData.newRawUri("File", uri)
                                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                                }
                        PreparedAttachmentExport(intent) { committed -> release(entry, committed) }.also { lease = it }
                    } catch (failure: Throwable) {
                        release(entry, false)
                        throw failure
                    }
                }
            }
        } catch (failure: Throwable) {
            // withContext can cancel after preparation, before it returns the lease to its caller.
            lease?.close()
            throw failure
        }
    }

    private fun release(
        entry: Entry,
        committed: Boolean,
    ) = synchronized(entries) {
        entry.committed = entry.committed || committed
        entry.leases--
        if (entry.leases == 0 && !entry.committed) {
            if (entry.file.delete() || !entry.file.exists()) entries.remove(entry.file.absolutePath, entry)
        }
    }

    fun forget(directory: File) =
        synchronized(entries) {
            entries.entries.removeAll { it.value.file.parentFile == directory }
        }
}
