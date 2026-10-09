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
    @Test fun signedOutColdSessionReplaysActualExportAndGrantCleanup() = coldCleanup(startupOnly = false)

    @Test fun signedOutStartupAloneReplaysActualExportAndGrantCleanup() = coldCleanup(startupOnly = true)

    private fun coldCleanup(startupOnly: Boolean) =
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
            var cleanupOwnedDraft: suspend () -> Unit = {}
            try {
                withTimeout(90_000) {
                    try {
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
                        val privateBytes = ByteArray(24) { (it % 251).toByte() }
                        val drafts =
                            AttachmentDraftCoordinator(
                                owner.key,
                                owner.client,
                                owner.paths,
                                session.preferences,
                                session.secrets,
                                SendCoordinator(session.preferences, { session.accepts(owner.key) }),
                                session::accepts,
                                { change -> session.admit(owner.key, change) },
                            )
                        val sourceUri = Uri.parse("content://$target.file-source/file?bytes=24&length=1")
                        val draft = drafts.select(context.contentResolver, sourceUri, "")
                        val draftId = draft.draftId
                        val secretRef = checkNotNull(draft.descriptorSecretRef)
                        val encoded = checkNotNull(session.secrets.read(owner.key.profileId, secretRef))
                        val privateRemote = AttachmentDescriptor.decode(encoded)
                        val privateFile = File(owner.client.attachments().localPath(privateRemote))
                        cleanupOwnedDraft = {
                            owner.client.attachments().deleteLocal(privateRemote)
                            owner.client.attachments().deleteLocal(upload.remoteAttachment())
                            session.preferences.removeDraft(owner.key.profileId, draftId)
                            session.secrets.delete(owner.key.profileId, secretRef)
                        }
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
                        // Finish the initial empty startup before testing the separate awaited path.
                        val waiting = if (startupOnly) null else AppSession(context)
                        waiting?.awaitStartupExportCleanup()
                        stopping = async(Dispatchers.IO) { runCatching { session.signOut() } }
                        withTimeout(30_000) { held.await() }
                        assertFalse(session.preferences.signedIn())
                        assertEquals(setOf(owner.key.profileId), session.preferences.pendingExportCleanup())
                        assertArrayEquals(bytes, exported.readBytes())
                        assertTrue("The actual external grant remains before withheld cleanup", canRead(uri))
                        val database = owner.paths.database
                        assertTrue(database.exists())
                        // Startup reconstructs after persistence.
                        // Restore replays a later journal on an unopened session.
                        val cold = waiting ?: AppSession(context)
                        if (startupOnly) cold.awaitStartupExportCleanup() else cold.restore()
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
                        // Only the test removes its own private draft after all preservation assertions.
                        cleanupOwnedDraft()
                        cleanupOwnedDraft = {}
                        releaseDrain.complete(Unit)
                        worker.join()
                        assertTrue(checkNotNull(stopping).await().isSuccess)
                        val proof = "ATTACHMENT_COLD_EXPORT_PROOF"
                        println(
                            "$proof persisted-death-gap=true external-uid=true startup=$startupOnly sdk-data-kept=true",
                        )
                    } finally {
                        withContext(NonCancellable) {
                            try {
                                cleanupOwnedDraft()
                            } finally {
                                releaseDrain.complete(Unit)
                            }
                        }
                    }
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
