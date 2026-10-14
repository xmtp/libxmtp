package org.xmtp.android.example.messenger

import android.app.Activity
import android.graphics.Bitmap
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File

internal fun saveMessengerScreenshot(
    activity: Activity,
    name: String,
) {
    require(name.matches(Regex("(?:scale-(?:(?:unreachable|clipped)-)?[a-z_]+|group-settings-pending-remove)")))
    val bitmap = checkNotNull(InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot())
    try {
        val directory = File(activity.filesDir, "xmtp-messenger-proof")
        check(directory.isDirectory || directory.mkdirs())
        File(directory, "$name.png").outputStream().use {
            check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it))
        }
    } finally {
        bitmap.recycle()
    }
}
