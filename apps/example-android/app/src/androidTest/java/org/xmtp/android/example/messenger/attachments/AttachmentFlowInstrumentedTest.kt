package org.xmtp.android.example.messenger.attachments

import android.net.Uri
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.xmtp_sdk.*
import java.io.File
import java.security.MessageDigest

@RunWith(AndroidJUnit4::class)
class AttachmentFlowInstrumentedTest {
    private val sourceAuthority: String
        get() {
            val context = InstrumentationRegistry.getInstrumentation().context
            return context.packageName + ".file-source"
        }

    @Test fun a64MiBProviderFileUploadsAndVerifiesOnAnotherClient() =
        runBlocking {
            val sender = AttachmentTestFixture()
            val receiver = AttachmentTestFixture()
            try {
                withTimeout(180_000) {
                    sender.start()
                    receiver.start()
                    val size = 64 * 1024 * 1024
                    val authority = sourceAuthority
                    val uri = Uri.parse("content://$authority/file?bytes=$size&length=1")
                    val coordinator = sender.coordinator()
                    val draft = coordinator.select(sender.context.contentResolver, uri, sender.group.id())
                    val ref = checkNotNull(draft.descriptorSecretRef)
                    val descriptor = checkNotNull(sender.secrets.read(sender.profile.id, ref))
                    val remote = AttachmentDescriptor.decode(descriptor)
                    coordinator.send(draft.draftId, sender.group) { }
                    assertEquals(1, sender.group.messages(null).size)
                    val downloaded = receiver.client.attachments().download(remote)
                    val file = File(downloaded.path)
                    assertEquals(size.toLong(), file.length())
                    val expected = MessageDigest.getInstance("SHA-256")
                    val pattern = ByteArray(8192) { (it % 251).toByte() }
                    repeat(size / pattern.size) { expected.update(pattern) }
                    val actual = MessageDigest.getInstance("SHA-256")
                    file.inputStream().use { input ->
                        val chunk = ByteArray(64 * 1024)
                        while (true) {
                            val count = input.read(chunk)
                            if (count < 0) break
                            actual.update(chunk, 0, count)
                        }
                    }
                    assertArrayEquals(expected.digest(), actual.digest())
                    assertTrue(
                        sender.paths.temp
                            .listFiles()
                            .orEmpty()
                            .isEmpty(),
                    )
                }
            } finally {
                receiver.close()
                sender.close()
            }
        }
}
