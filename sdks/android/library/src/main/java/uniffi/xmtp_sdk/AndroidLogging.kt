package uniffi.xmtp_sdk

import android.content.Context
import java.io.File

/** Write native logs in the app's files directory. */
fun SDKClient.Companion.activatePersistentLibXMTPLogWriter(
    context: Context,
    logLevel: LogLevel,
    rotationSchedule: LogRotation,
    maxFiles: UInt,
    processType: LogProcessType = LogProcessType.MAIN,
) {
    val directory = File(context.applicationContext.filesDir, "xmtp_logs")
    check(directory.isDirectory || directory.mkdirs()) { "Cannot create XMTP log directory" }
    enterDebugWriter(directory.absolutePath, rotationSchedule, maxFiles, logLevel, processType)
}

fun SDKClient.Companion.deactivatePersistentLibXMTPLogWriter() = exitDebugWriter()

fun SDKClient.Companion.getXMTPLogFilePaths(context: Context): List<String> =
    File(context.applicationContext.filesDir, "xmtp_logs")
        .listFiles()
        .orEmpty()
        .filter { it.isFile }
        .map { it.absolutePath }

/** Stop the file writer and return the number of files deleted. */
fun SDKClient.Companion.clearXMTPLogs(context: Context): Int {
    exitDebugWriter()
    return getXMTPLogFilePaths(context).count { File(it).delete() }
}
