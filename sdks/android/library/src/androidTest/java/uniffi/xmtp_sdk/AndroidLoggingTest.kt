package uniffi.xmtp_sdk

import android.content.Context
import android.content.ContextWrapper
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID

/** Run this class alone in a fresh process to check an uninitialized native logger. */
@RunWith(AndroidJUnit4::class)
class AndroidLoggingTest {
    @Test
    fun clearsPersistedLogsWithoutInitializingLogging() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.cacheDir, "log-clear-${UUID.randomUUID()}")
        val testContext =
            object : ContextWrapper(context) {
                override fun getFilesDir(): File = root

                override fun getApplicationContext(): Context = this
            }
        val directory = File(root, "xmtp_logs")
        assertTrue(directory.mkdirs())
        val marker = File(directory, "previous-process.log").apply { writeText("persisted log") }
        try {
            assertEquals(1, SDKClient.clearXMTPLogs(testContext))
            assertFalse(marker.exists())
            assertTrue(directory.delete())
            assertEquals(0, SDKClient.clearXMTPLogs(testContext))
        } finally {
            root.deleteRecursively()
        }
    }
}
