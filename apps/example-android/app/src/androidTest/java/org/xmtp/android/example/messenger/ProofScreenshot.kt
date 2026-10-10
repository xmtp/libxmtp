package org.xmtp.android.example.messenger

import android.content.Context
import android.graphics.Bitmap
import java.io.File

/** Test artifacts stay in the app's private files until the owned fixture exports them. */
internal fun writeProofScreenshot(
    context: Context,
    name: String,
    bitmap: Bitmap,
): File {
    require(Regex("[a-zA-Z0-9_-]+").matches(name))
    val directory = File(context.filesDir, "xmtp-messenger-proof")
    check(directory.mkdirs() || directory.isDirectory)
    return File(directory, "$name.png").also { file ->
        file.outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) }
    }
}
