package org.xmtp.android.example.messenger.attachments

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

/** A verified result can enter this class only through the SDK download operation. */
class AttachmentFiles(
    private val context: Context,
    private val key: SessionKey,
    private val client: SDKClient,
    private val accepts: (SessionKey) -> Boolean,
) {
    private val verified = VerifiedAttachmentCache<DownloadedAttachment>()
    internal val retained = verified.retained
    internal var beforeDownloadAdmission: suspend () -> Unit = {}
    internal var beforeOpenCopy: suspend () -> Unit = {}
    internal var beforeSaveCopy: suspend () -> Unit = {}

    internal fun clearVerified() = verified.clear()

    private fun checkCurrent() = check(accepts(key)) { "The session changed" }

    suspend fun download(
        messageId: String,
        remote: RemoteAttachment,
        admission: () -> Boolean = { true },
    ): DownloadedAttachment {
        checkCurrent()
        check(admission()) { "The file view changed" }
        val generation = verified.generation()
        val downloaded = client.attachments().download(remote)
        beforeDownloadAdmission()
        checkCurrent()
        check(verified.admit(generation, messageId, downloaded, admission)) { "The file view changed" }
        return downloaded
    }

    suspend fun preview(messageId: String): Bitmap? =
        preview(
            checkNotNull(verified[messageId]) {
                "Download the file first"
            },
        )

    internal suspend fun preview(downloaded: DownloadedAttachment): Bitmap? =
        withContext(Dispatchers.IO) {
            checkCurrent()
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

    internal fun verifiedResult(messageId: String): DownloadedAttachment? =
        if (accepts(key)) verified[messageId] else null

    suspend fun openIntent(messageId: String): Intent =
        openIntent(
            checkNotNull(verifiedResult(messageId)) {
                "Download the file first"
            },
        )

    internal suspend fun openIntent(downloaded: DownloadedAttachment): Intent {
        val export = prepareOpen(downloaded)
        // Callers of this compatibility API own the returned Intent and its viewer lifetime.
        export.launched()
        return export.intent
    }

    internal suspend fun prepareOpen(
        downloaded: DownloadedAttachment,
        admission: () -> Boolean = { true },
    ): PreparedAttachmentExport {
        checkCurrent()
        beforeOpenCopy()
        return AttachmentExports.prepare(context, key.profileId, downloaded) { accepts(key) && admission() }
    }

    suspend fun save(
        messageId: String,
        destination: Uri,
    ) = save(checkNotNull(verifiedResult(messageId)) { "Download the file first" }, destination)

    internal suspend fun save(
        downloaded: DownloadedAttachment,
        destination: Uri,
    ) {
        checkCurrent()
        beforeSaveCopy()
        withContext(Dispatchers.IO) {
            checkCurrent()
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
    }

    fun filename(messageId: String) = verified[messageId]?.filename ?: "File"

    fun isDownloaded(messageId: String) = messageId in retained.value

    companion object {
        /** Call after session work drains. Immediate revocation is a separate step. */
        fun removeProfileExports(
            context: Context,
            profileId: String,
        ) {
            revokeProfile(context, profileId)
            val directory = profileDirectory(context, profileId)
            check(!directory.exists() || directory.deleteRecursively()) { "Cannot clear file exports" }
            AttachmentExports.forget(directory)
        }

        fun profileDirectory(
            context: Context,
            profileId: String,
        ): File {
            require(profileId.matches(Regex("[a-zA-Z0-9-]+")))
            return File(context.filesDir, "messenger-exports/$profileId")
        }

        /** Revoke before owner drain, then repeat before the export files are removed. */
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
