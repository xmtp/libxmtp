package org.xmtp.android.example.messenger.attachments

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.net.Uri
import android.os.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.*
import uniffi.xmtp_sdk.*
import java.io.File
import java.util.UUID
import android.os.Message as AndroidMessage

class AttachmentColdCleanupInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    /** Reconstructs the durable death gap. It does not kill the Android process. */
    @Test fun signedOutColdSessionReplaysActualExportAndGrantCleanup() =
        runBlocking<Unit> {
            val instrumentation = InstrumentationRegistry.getInstrumentation()
            val context = instrumentation.targetContext
            val target = instrumentation.context.packageName
            val session = model.session
            val lifecycle = AndroidStreamLifecycle.enabled
            val invalidated = session.onInvalidated
            val message = session.onMessage
            val sender = AttachmentTestFixture()
            val releaseDrain = CompletableDeferred<Unit>()
            var connection: ServiceConnection? = null
            var bound = false
            var directory: File? = null
            var stopping: Deferred<Result<Unit>>? = null
            try {
                withTimeout(90_000) {
                    AndroidStreamLifecycle.enabled = false
                    resumeStreams()
                    session.signOut()
                    session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
                    val owner = checkNotNull(session.active.value)
                    model.foreground(false)
                    model.featureRefresh(owner, null)
                    sender.start()
                    val bytes = "real cold export".toByteArray()
                    val source = AttachmentSource.Bytes(bytes, "cold.txt", "text/plain")
                    val upload = sender.client.attachments().create(source)
                    upload.upload()
                    val files =
                        AttachmentFiles(context, owner.key, owner.client, session::accepts)
                    val downloaded = files.download("file", upload.remoteAttachment())
                    val cached = File(downloaded.path)
                    assertArrayEquals(bytes, cached.readBytes())
                    val intent = files.openIntent("file")
                    val uri = checkNotNull(intent.data)
                    val exports = AttachmentFiles.profileDirectory(context, owner.key.profileId)
                    directory = exports
                    val exported = exports.listFiles()!!.single()
                    assertArrayEquals(bytes, exported.readBytes())
                    val opened = CompletableDeferred<Pair<Boolean, Int>>()
                    val response =
                        object : ResultReceiver(Handler(Looper.getMainLooper())) {
                            override fun onReceiveResult(
                                resultCode: Int,
                                resultData: Bundle?,
                            ) {
                                opened.complete((resultCode == 1) to (resultData?.getInt("uid") ?: -1))
                            }
                        }
                    context.startActivity(
                        intent
                            .setComponent(ComponentName(target, FileGrantReceiverActivity::class.java.name))
                            .putExtra("result", response)
                            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                    )
                    val activityRead = withTimeout(10_000) { opened.await() }
                    assertTrue(activityRead.first)
                    assertNotEquals(android.os.Process.myUid(), activityRead.second)
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
                        context.bindService(
                            Intent().setComponent(ComponentName(target, FileGrantReceiverService::class.java.name)),
                            checkNotNull(connection),
                            Context.BIND_AUTO_CREATE,
                        )
                    assertTrue(bound)
                    val external = withTimeout(10_000) { service.await() }

                    suspend fun canRead(exact: Uri): Boolean {
                        val read = CompletableDeferred<Pair<Boolean, Int>>()
                        val reply =
                            Messenger(
                                object : Handler(Looper.getMainLooper()) {
                                    override fun handleMessage(message: AndroidMessage) {
                                        read.complete((message.arg1 == 1) to message.data.getInt("uid"))
                                    }
                                },
                            )
                        external.send(
                            AndroidMessage.obtain().apply {
                                data = Bundle().apply { putString("uri", exact.toString()) }
                                replyTo = reply
                            },
                        )
                        val state = withTimeout(10_000) { read.await() }
                        assertEquals(activityRead.second, state.second)
                        return state.first
                    }
                    context.grantUriPermission(target, uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    assertTrue(canRead(uri))
                    session.onInvalidated = {}
                    session.onMessage = { _, _ -> }
                    val privateBytes = "preserved private draft".toByteArray()
                    val privatePending =
                        owner.client.attachments().create(
                            AttachmentSource.Bytes(privateBytes, "private.txt", "text/plain"),
                        )
                    val privateRemote = privatePending.remoteAttachment()
                    val privateFile = File(owner.client.attachments().localPath(privateRemote))
                    val draftId = UUID.randomUUID().toString()
                    val secretRef = "attachment-$draftId"
                    val draft = SendDraftRef(draftId, "", secretRef)
                    val encoded = AttachmentDescriptor.encode(privateRemote)
                    session.secrets.write(owner.key.profileId, secretRef, encoded)
                    session.preferences.saveDraft(owner.key.profileId, draft)
                    assertArrayEquals(privateBytes, privateFile.readBytes())
                    val entered = CompletableDeferred<Unit>()
                    val held = CompletableDeferred<Unit>()
                    val worker =
                        owner.work.launch {
                            entered.complete(Unit)
                            try {
                                awaitCancellation()
                            } finally {
                                withContext(NonCancellable) {
                                    held.complete(Unit)
                                    releaseDrain.await()
                                }
                            }
                        }
                    entered.await()
                    stopping = async(Dispatchers.IO) { runCatching { session.signOut() } }
                    withTimeout(30_000) { held.await() }
                    assertFalse(session.preferences.signedIn())
                    assertEquals(setOf(owner.key.profileId), session.preferences.pendingExportCleanup())
                    assertArrayEquals(bytes, exported.readBytes())
                    assertTrue("The actual external grant remains before withheld cleanup", canRead(uri))
                    val database = owner.paths.database
                    assertTrue(database.exists())
                    // The original process cleanup is held while a new session reads its durable journal.
                    val cold = AppSession(context)
                    cold.restore()
                    assertNull("Signed-out cold recovery does not open an SDK owner", cold.active.value)
                    assertFalse("Cold replay deletes the recorded profile export", exports.exists())
                    assertFalse(canRead(uri))
                    exports.mkdirs()
                    File(exports, exported.name).writeBytes(bytes)
                    assertFalse("Cold replay revoked the old URI grant, not only the old file", canRead(uri))
                    assertTrue(database.exists())
                    assertArrayEquals("SDK cached bytes survive export-only replay", bytes, cached.readBytes())
                    assertTrue(cold.preferences.drafts(owner.key.profileId).contains(draft))
                    assertArrayEquals(encoded, cold.secrets.read(owner.key.profileId, secretRef))
                    assertArrayEquals(privateBytes, privateFile.readBytes())
                    assertTrue(cold.preferences.pendingExportCleanup().isEmpty())
                    cold.restore()
                    releaseDrain.complete(Unit)
                    worker.join()
                    assertTrue(checkNotNull(stopping).await().isSuccess)
                    val proof = "ATTACHMENT_COLD_EXPORT_PROOF"
                    println("$proof persisted-death-gap=true external-uid=true replay=true sdk-data-kept=true")
                }
            } finally {
                releaseDrain.complete(Unit)
                if (bound) context.unbindService(checkNotNull(connection))
                withContext(NonCancellable) {
                    stopping?.await()
                    directory?.let {
                        AttachmentFiles.revokeProfile(context, it.name)
                        it.deleteRecursively()
                    }
                    sender.close()
                    session.onInvalidated = invalidated
                    session.onMessage = message
                    if (session.active.value != null || session.preferences.reset() != null) session.deleteAccount()
                    session.signOut()
                    AndroidStreamLifecycle.enabled = lifecycle
                }
            }
        }
}
