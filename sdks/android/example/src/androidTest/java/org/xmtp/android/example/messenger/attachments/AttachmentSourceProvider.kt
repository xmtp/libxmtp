package org.xmtp.android.example.messenger.attachments

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns

/** A test provider whose size hint can differ from its stream. */
class AttachmentSourceProvider : ContentProvider() {
    override fun onCreate() = true

    override fun getType(uri: Uri) = "application/octet-stream"

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor {
        val columns = projection ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
        return MatrixCursor(columns).apply {
            addRow(
                columns
                    .map {
                        if (it ==
                            OpenableColumns.DISPLAY_NAME
                        ) {
                            "../../same.bin"
                        } else {
                            uri.getQueryParameter("length")?.toLong()
                        }
                    }.toTypedArray(),
            )
        }
    }

    override fun openFile(
        uri: Uri,
        mode: String,
    ): ParcelFileDescriptor {
        require(mode == "r")
        val size = checkNotNull(uri.getQueryParameter("bytes")).toInt()
        require(size in 0..1_048_576)
        val pipe = ParcelFileDescriptor.createPipe()
        Thread {
            try {
                ParcelFileDescriptor.AutoCloseOutputStream(pipe[1]).use { output ->
                    val chunk = ByteArray(8192) { (it % 251).toByte() }
                    var left = size
                    while (left >
                        0
                    ) {
                        val count = minOf(left, chunk.size)
                        output.write(chunk, 0, count)
                        left -= count
                    }
                }
            } catch (_: java.io.IOException) {
                // The app can close an oversized source.
            }
        }.start()
        return pipe[0]
    }

    override fun insert(
        uri: Uri,
        values: ContentValues?,
    ): Uri? = error("Read only")

    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<out String>?,
    ) = 0

    override fun delete(
        uri: Uri,
        selection: String?,
        selectionArgs: Array<out String>?,
    ) = 0
}
