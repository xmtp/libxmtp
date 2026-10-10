package org.xmtp.android.example.messenger.attachments

import androidx.core.content.FileProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.xmtp_sdk.*
import java.io.File

@RunWith(AndroidJUnit4::class)
class VerifiedAttachmentLifetimeInstrumentedTest {
    @Test fun metadataEvictionKeepsCapturedOpenAndSaveAndRejectsLateSdkCompletion() =
        runBlocking<Unit> {
            val fixture = AttachmentTestFixture()
            val release = CompletableDeferred<Unit>()
            try {
                withTimeout(90_000) {
                    fixture.start()
                    val bytes = "verified cache lifetime".toByteArray()
                    val pending =
                        fixture.client.attachments().create(
                            AttachmentSource.Bytes(bytes, "cache.txt", "text/plain"),
                        )
                    pending.upload()
                    val remote = pending.remoteAttachment()
                    val files = AttachmentFiles(fixture.context, fixture.key, fixture.client) { fixture.current }
                    val downloaded = files.download("active", remote)
                    val sdkFile = File(downloaded.path)
                    assertArrayEquals(bytes, sdkFile.readBytes())
                    val directory = AttachmentFiles.profileDirectory(fixture.context, fixture.profile.id)
                    directory.mkdirs()
                    val saved = File(directory, "saved")
                    val destination =
                        FileProvider.getUriForFile(
                            fixture.context,
                            "${fixture.context.packageName}.fileprovider",
                            saved,
                        )
                    val opening = CompletableDeferred<Unit>()
                    val saving = CompletableDeferred<Unit>()
                    files.beforeOpenCopy = {
                        opening.complete(Unit)
                        release.await()
                    }
                    files.beforeSaveCopy = {
                        saving.complete(Unit)
                        release.await()
                    }
                    val open = async { runCatching { files.openIntent("active") } }
                    val save = async { runCatching { files.save("active", destination) } }
                    opening.await()
                    saving.await()
                    repeat(VerifiedAttachmentCache.MAX_RECORDS) { files.download("other-$it", remote) }
                    assertFalse("The oldest metadata record is evicted", files.isDownloaded("active"))
                    assertEquals(VerifiedAttachmentCache.MAX_RECORDS, files.retained.value.size)
                    files.clearVerified()
                    assertTrue(files.retained.value.isEmpty())
                    release.complete(Unit)
                    val savedResult = save.await()
                    val openResult = open.await()
                    assertTrue(
                        "Captured Save completes after metadata eviction and clear: $savedResult",
                        savedResult.isSuccess,
                    )
                    assertTrue(
                        "Captured Open completes after metadata eviction and clear: $openResult",
                        openResult.isSuccess,
                    )
                    val exported = checkNotNull(openResult.getOrThrow().data)
                    fixture.context.contentResolver
                        .openInputStream(
                            exported,
                        )!!
                        .use { assertArrayEquals(bytes, it.readBytes()) }
                    assertArrayEquals(bytes, saved.readBytes())
                    val completed = CompletableDeferred<Unit>()
                    val admit = CompletableDeferred<Unit>()
                    files.beforeDownloadAdmission = {
                        completed.complete(Unit)
                        admit.await()
                    }
                    val late = async { runCatching { files.download("late", remote) } }
                    completed.await()
                    files.clearVerified()
                    admit.complete(Unit)
                    assertEquals("The file view changed", late.await().exceptionOrNull()?.message)
                    assertFalse("Actual SDK completion cannot refill cleared metadata", files.isDownloaded("late"))
                    assertArrayEquals("Metadata cleanup keeps the SDK cached file", bytes, sdkFile.readBytes())
                    files.beforeDownloadAdmission = {}
                    files.download("fresh", remote)
                    assertEquals(setOf("fresh"), files.retained.value)
                    println(
                        "ATTACHMENT_VERIFIED_LIFETIME_PROOF bounded=true active-open-save=true late-sdk-rejected=true sdk-file-kept=true",
                    )
                }
            } finally {
                release.complete(Unit)
                fixture.close()
            }
        }
}
