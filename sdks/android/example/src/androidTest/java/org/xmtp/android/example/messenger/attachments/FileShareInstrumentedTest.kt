package org.xmtp.android.example.messenger.attachments

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.net.Uri
import android.os.*
import androidx.core.content.FileProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.example.messenger.AppSession
import uniffi.xmtp_sdk.*
import java.io.File
import android.os.Message as AndroidMessage

@RunWith(AndroidJUnit4::class)
class FileShareInstrumentedTest {
    @Test fun externalReceiverReadsOnlyTheGrantedFileAndResetRevokesIt() =
        runBlocking<Unit> {
            val fixture = AttachmentTestFixture()
            val instrumentation = InstrumentationRegistry.getInstrumentation()
            val target = instrumentation.context.packageName
            var bound = false
            var connection: ServiceConnection? = null
            try {
                withTimeout(90_000) {
                    fixture.start()
                    val pending =
                        fixture.client.attachments().create(
                            AttachmentSource.Bytes("grant".toByteArray(), "label.txt", "text/plain"),
                        )
                    pending.upload()
                    val files = AttachmentFiles(fixture.context, fixture.key, fixture.client) { fixture.current }
                    files.download("file", pending.remoteAttachment())
                    val open = files.openIntent("file")
                    val exact = checkNotNull(open.data)
                    val exportRoot = AttachmentFiles.profileDirectory(fixture.context, fixture.profile.id)
                    val sibling = File(exportRoot, "sibling").apply { writeText("sibling") }
                    val siblingUri =
                        FileProvider.getUriForFile(
                            fixture.context,
                            "${fixture.context.packageName}.fileprovider",
                            sibling,
                        )
                    files.save("file", siblingUri)
                    assertEquals("grant", sibling.readText())
                    android.util.Log.i("XmtpFileGrant", "Launch external receiver")
                    val opened = CompletableDeferred<Pair<Boolean, Int>>()
                    val result =
                        object : ResultReceiver(Handler(Looper.getMainLooper())) {
                            override fun onReceiveResult(
                                resultCode: Int,
                                resultData: Bundle?,
                            ) {
                                opened.complete((resultCode == 1) to (resultData?.getInt("uid") ?: -1))
                            }
                        }
                    fixture.context.startActivity(
                        open
                            .setComponent(
                                ComponentName(target, FileGrantReceiverActivity::class.java.name),
                            ).putExtra("result", result)
                            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                    )
                    val resultState = withTimeout(10_000) { opened.await() }
                    assertTrue(resultState.first)
                    assertNotEquals(android.os.Process.myUid(), resultState.second)
                    android.util.Log.i("XmtpFileGrant", "Receiver result received from another UID")
                    val service = CompletableDeferred<Messenger>()
                    connection =
                        object : ServiceConnection {
                            override fun onServiceConnected(
                                name: ComponentName,
                                binder: IBinder,
                            ) {
                                service.complete(Messenger(binder))
                            }

                            override fun onServiceDisconnected(name: ComponentName) { }
                        }
                    bound =
                        fixture.context.bindService(
                            Intent().setComponent(ComponentName(target, FileGrantReceiverService::class.java.name)),
                            checkNotNull(connection),
                            Context.BIND_AUTO_CREATE,
                        )
                    assertTrue(bound)
                    val remote = withTimeout(10_000) { service.await() }

                    suspend fun canRead(uri: Uri): Boolean {
                        val read = CompletableDeferred<Pair<Boolean, Int>>()
                        val response =
                            Messenger(
                                object : Handler(Looper.getMainLooper()) {
                                    override fun handleMessage(message: AndroidMessage) {
                                        read.complete((message.arg1 == 1) to message.data.getInt("uid"))
                                    }
                                },
                            )
                        remote.send(
                            AndroidMessage.obtain().apply {
                                data = Bundle().apply { putString("uri", uri.toString()) }
                                replyTo =
                                    response
                            },
                        )
                        val state = withTimeout(10_000) { read.await() }
                        assertNotEquals(android.os.Process.myUid(), state.second)
                        return state.first
                    }
                    assertFalse(canRead(siblingUri))
                    android.util.Log.i("XmtpFileGrant", "Sibling denied; check explicit grant and revoke")
                    // Grant it explicitly to verify the service route before the revoke check.
                    fixture.context.grantUriPermission(target, exact, Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    assertTrue(canRead(exact))
                    AttachmentFiles.revokeProfile(fixture.context, fixture.profile.id)
                    assertFalse(canRead(exact))
                    fixture.context.grantUriPermission(target, exact, Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    assertTrue(canRead(exact))
                    fixture.current = false
                    fixture.endClient()
                    fixture.preferences.setActive(fixture.profile)
                    fixture.preferences.saveReset(fixture.paths.resetRecord(fixture.profile.id))
                    android.util.Log.i("XmtpFileGrant", "Recover recorded STOPPING reset")
                    AppSession(fixture.context).restore()
                    assertFalse(exportRoot.exists())
                    assertFalse(canRead(exact))
                    // A removed file alone does not prove that the old URI grant was revoked.
                    exportRoot.mkdirs()
                    val exportName = exact.lastPathSegment!!
                    File(exportRoot, exportName).writeText("replacement")
                    assertFalse(canRead(exact))
                    android.util.Log.i("XmtpFileGrant", "Same-path replacement denied after reset")
                }
            } finally {
                if (bound) fixture.context.unbindService(checkNotNull(connection))
                fixture.close()
            }
        }
}
