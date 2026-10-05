package org.xmtp.android.example

import android.content.Context
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import uniffi.xmtp_sdk.*

/** Keep the process-wide writer independent of an Activity's lifetime. */
internal object PersistentLogs {
    // Use Main here so the controller's IO context change dispatches native work.
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var controller: LogWriterController? = null

    @Synchronized
    fun controller(context: Context): LogWriterController =
        controller ?: run {
            val application = context.applicationContext
            val preferences = application.getSharedPreferences("XMTPPreferences", Context.MODE_PRIVATE)
            LogWriterController(
                scope = scope,
                activate = {
                    initLogging(LoggingOptions(level = LogLevel.DEBUG))
                    SDKClient.activatePersistentLibXMTPLogWriter(
                        application,
                        LogLevel.DEBUG,
                        LogRotation.MINUTELY,
                        3u,
                    )
                },
                deactivate = { SDKClient.deactivatePersistentLibXMTPLogWriter() },
                isActivated = { preferences.getBoolean("logs_activated", false) },
                saveActivated = { preferences.edit().putBoolean("logs_activated", it).apply() },
            ).also { controller = it }
        }
}
