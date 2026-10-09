package org.xmtp.android.example.messenger

import android.os.Build
import android.os.ParcelFileDescriptor
import androidx.test.platform.app.InstrumentationRegistry
import java.util.UUID

internal fun checkedScreenShell(command: String): String {
    if (Build.VERSION.SDK_INT < 31) error("Screen shell proof requires API 31 or newer")
    val marker = "XMTP_EXIT_" + UUID.randomUUID().toString().replace("-", "")
    val pipes = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommandRw("sh")
    val result =
        pipes[0].use { output ->
            pipes[1].use { input ->
                ParcelFileDescriptor.AutoCloseOutputStream(input).use { stream ->
                    stream.write(screenShellScript(command, marker).toByteArray())
                }
                ParcelFileDescriptor.AutoCloseInputStream(output).bufferedReader().use { stream ->
                    screenShellOutput(stream.readText(), marker)
                }
            }
        }
    println("SCREEN_SHELL_COMMAND $command => $result")
    return result
}
