package org.xmtp.android.example.messenger.attachments

import android.content.ContentResolver
import android.net.Uri
import android.provider.OpenableColumns
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext

internal data class PrivateSource(val file: File, val filename: String?, val mimeType: String)

internal object PrivateFileStager {
    const val CHUNK_BYTES = 64 * 1024

    suspend fun copy(input: InputStream, output: OutputStream, ceiling: ULong): ULong {
        val buffer = ByteArray(CHUNK_BYTES)
        var count = 0uL
        while (true) {
            currentCoroutineContext().ensureActive()
            val read = input.read(buffer)
            if (read < 0) return count
            if (read == 0) continue
            require(read.toULong() <= ceiling - count) { "File exceeds the server upload limit" }
            output.write(buffer, 0, read)
            count += read.toULong()
        }
    }

    suspend fun stage(resolver: ContentResolver, uri: Uri, directory: File, ceiling: ULong): PrivateSource = withContext(Dispatchers.IO) {
        require(uri.scheme == "content") { "Select a file from the system picker" }
        check(directory.isDirectory || directory.mkdirs()) { "Cannot create the private source directory" }
        val file = File(directory, "source-${UUID.randomUUID()}")
        try {
            // Names are labels. The provider size does not determine the copy limit.
            val label = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) cursor.getString(0)?.take(255) else null
            }
            val mime = resolver.getType(uri) ?: "application/octet-stream"
            checkNotNull(resolver.openInputStream(uri)) { "Cannot read the selected file" }.use { input ->
                file.outputStream().use { output -> copy(input, output, ceiling) }
            }
            PrivateSource(file, label, mime)
        } catch (error: Throwable) {
            file.delete()
            throw error
        }
    }

    fun sweep(directory: File) {
        directory.listFiles()?.filter { it.name.startsWith("source-") }?.forEach { check(it.delete()) { "Cannot remove a private source" } }
    }
}
