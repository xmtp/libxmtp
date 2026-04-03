package org.xmtp.android.library

import android.content.Context
import java.io.File

object AndroidClientHelper {
    fun defaultDbDirectory(context: Context): String =
        File(context.filesDir.absolutePath, "xmtp_db").absolutePath

    fun defaultLogDirectory(context: Context): String =
        File(context.filesDir, "xmtp_logs").absolutePath
}
