package org.xmtp.android.example.messenger.attachments

import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.xmtp.android.example.messenger.SessionKey
import uniffi.xmtp_sdk.DownloadedAttachment
import uniffi.xmtp_sdk.RemoteAttachment
import uniffi.xmtp_sdk.SDKClient
import uniffi.xmtp_sdk.attachments
import java.io.File
import java.util.UUID

/** A verified result can enter this class only through the SDK download operation. */
class AttachmentFiles(
    private val context: Context,
    private val key: SessionKey,
    private val client: SDKClient,
    private val accepts: (SessionKey) -> Boolean,
) {
    private val verified = java.util.concurrent.ConcurrentHashMap<String, DownloadedAttachment>()

    private fun checkCurrent() = check(accepts(key)) { "The session changed" }

    suspend fun download(
        messageId: String,
        remote: RemoteAttachment,
    ): DownloadedAttachment {
        checkCurrent()
        val downloaded = client.attachments().download(remote)
        checkCurrent()
        verified[messageId] = downloaded
        return downloaded
    }

    suspend fun preview(messageId: String): Bitmap? =
        withContext(Dispatchers.IO) {
            checkCurrent()
            val downloaded = checkNotNull(verified[messageId]) { "Download the file first" }
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(downloaded.path, bounds)
            if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return@withContext null
            var sample = 1
            while (bounds.outWidth / sample > 1024 || bounds.outHeight / sample > 1024) sample *= 2
            val settings = BitmapFactory.Options().apply { inSampleSize = sample }
            val bitmap = BitmapFactory.decodeFile(downloaded.path, settings)
            val current = accepts(key)
            if (!current) bitmap?.recycle()
            bitmap.takeIf { current }
        }

    suspend fun openIntent(messageId: String): Intent =
        withContext(Dispatchers.IO) {
            checkCurrent()
            val downloaded = checkNotNull(verified[messageId]) { "Download the file first" }
            val directory = profileDirectory(context, key.profileId)
            check(directory.isDirectory || directory.mkdirs()) { "Cannot create the export directory" }
            // The remote filename does not enter the path.
            val file = File(directory, "file-${UUID.randomUUID()}")
            try {
                val source = File(downloaded.path)
                source.inputStream().use { input ->
                    file.outputStream().use { output ->
                        PrivateFileStager.copy(input, output, ULong.MAX_VALUE)
                    }
                }
                checkCurrent()
                val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
                val mime = downloaded.mimeType ?: "application/octet-stream"
                Intent(Intent.ACTION_VIEW).setDataAndType(uri, mime).apply {
                    clipData = ClipData.newRawUri("File", uri)
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
            } catch (error: Throwable) {
                file.delete()
                throw error
            }
        }

    suspend fun save(
        messageId: String,
        destination: Uri,
    ) = withContext(Dispatchers.IO) {
        checkCurrent()
        val downloaded = checkNotNull(verified[messageId]) { "Download the file first" }
        File(downloaded.path).inputStream().use { input ->
            val target =
                context.contentResolver.openOutputStream(destination, "wt")
                    ?: error("Cannot write the selected destination")
            target.use { output ->
                PrivateFileStager.copy(input, output, ULong.MAX_VALUE)
            }
        }
        checkCurrent()
    }

    fun filename(messageId: String) = verified[messageId]?.filename ?: "File"

    fun isDownloaded(messageId: String) = verified.containsKey(messageId)

    companion object {
        fun profileDirectory(
            context: Context,
            profileId: String,
        ): File {
            require(profileId.matches(Regex("[a-zA-Z0-9-]+")))
            return File(context.filesDir, "messenger-exports/$profileId")
        }

        /** Call after all profile file actions have stopped, before reset removes files. */
        fun revokeProfile(
            context: Context,
            profileId: String,
        ) {
            profileDirectory(context, profileId).listFiles()?.forEach { file ->
                val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
                context.revokeUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
        }
    }
}
