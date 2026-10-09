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
import androidx.test.runner.lifecycle.ActivityLifecycleCallback
import androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry
import androidx.test.runner.lifecycle.Stage
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.*
import uniffi.xmtp_sdk.*
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import android.os.Message as AndroidMessage

class AttachmentSignOutInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    @Test fun ownerExportsAreRevokedWhenHostClosesDuringSignOutDrain() =
        runBlocking<Unit> {
            val instrumentation = InstrumentationRegistry.getInstrumentation()
            val context = instrumentation.targetContext
            val target = instrumentation.context.packageName
            val enabled = AndroidStreamLifecycle.enabled
            val sender = AttachmentTestFixture()
            val session = model.session
            val draining = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            val stopped = CountDownLatch(1)
            val lifecycle = ActivityLifecycleMonitorRegistry.getInstance()
            var callback: ActivityLifecycleCallback? = null
            var connection: ServiceConnection? = null
            var bound = false
            var exportRoot: File? = null
            try {
                withTimeout(90_000) {
                    AndroidStreamLifecycle.enabled = false
                    resumeStreams()
                    session.signOut()
                    session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
                    val owner = checkNotNull(session.active.value)
                    model.foreground(false)
                    sender.start()
                    val bytes = "actual external export before sign out".toByteArray()
                    val pending =
                        sender.client.attachments().create(
                            AttachmentSource.Bytes(bytes, "export.txt", "text/plain"),
                        )
                    pending.upload()
                    val remote = pending.remoteAttachment()
                    val files = AttachmentFiles(context, owner.key, owner.client, session::accepts)
                    val downloaded = files.download("file", remote)
                    assertArrayEquals(bytes, File(downloaded.path).readBytes())
                    val intent = files.openIntent("file")
                    val uri = checkNotNull(intent.data)
                    val directory = AttachmentFiles.profileDirectory(context, owner.key.profileId)
                    exportRoot = directory
                    assertEquals(owner.paths.exports, directory)
                    val exported = directory.listFiles()!!.single()
                    assertArrayEquals(bytes, exported.readBytes())
                    val opened = CompletableDeferred<Pair<Boolean, Int>>()
                    val receiver =
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
                            .putExtra("result", receiver)
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
                        val response =
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
                                replyTo = response
                            },
                        )
                        val result = withTimeout(10_000) { read.await() }
                        assertEquals(activityRead.second, result.second)
                        assertNotEquals(android.os.Process.myUid(), result.second)
                        return result.first
                    }
                    context.grantUriPermission(target, uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    assertTrue(canRead(uri))
                    val held = CompletableDeferred<Unit>()
                    val worker =
                        owner.work.launch {
                            held.complete(Unit)
                            try {
                                awaitCancellation()
                            } finally {
                                withContext(NonCancellable) {
                                    draining.complete(Unit)
                                    release.await()
                                }
                            }
                        }
                    held.await()
                    val signingOut =
                        async(Dispatchers.IO) {
                            try {
                                runCatching { session.signOut() }
                            } finally {
                                stopped.countDown()
                            }
                        }
                    withTimeout(30_000) { draining.await() }
                    assertNull(session.active.value)
                    assertTrue("Existing external grant stays readable during the held owner drain", canRead(uri))
                    assertArrayEquals(bytes, exported.readBytes())
                    val originalModel = model
                    val oldActivity = compose.activity
                    val recreated = CompletableDeferred<MainActivity>()
                    callback =
                        ActivityLifecycleCallback { activity, state ->
                            if (activity === oldActivity && state == Stage.DESTROYED) {
                                release.complete(Unit)
                                check(
                                    stopped.await(30, TimeUnit.SECONDS),
                                ) { "Sign out did not finish during host teardown" }
                            }
                            if (activity is MainActivity && activity !== oldActivity && state == Stage.STARTED) {
                                recreated.complete(activity)
                            }
                        }
                    lifecycle.addLifecycleCallback(checkNotNull(callback))
                    instrumentation.runOnMainSync { oldActivity.recreate() }
                    val newActivity = withTimeout(30_000) { recreated.await() }
                    assertSame(originalModel, ViewModelProvider(newActivity)[MessengerViewModel::class.java])
                    val signOut = signingOut.await()
                    assertTrue(
                        "Actual sign out completes during the Activity teardown gap: $signOut",
                        signOut.isSuccess,
                    )
                    worker.join()
                    assertFalse("Sign out removes exported plaintext after the old Host closes", directory.exists())
                    assertFalse(canRead(uri))
                    directory.mkdirs()
                    File(directory, exported.name).writeBytes(bytes)
                    assertFalse("The old external UID grant cannot read a same-path replacement", canRead(uri))
                    println(
                        "ATTACHMENT_SIGN_OUT_PROOF external-uid=true host-detached=true exports-removed=true grant-revoked=true",
                    )
                }
            } finally {
                release.complete(Unit)
                callback?.let(lifecycle::removeLifecycleCallback)
                if (bound) context.unbindService(checkNotNull(connection))
                withContext(NonCancellable) {
                    exportRoot?.let {
                        AttachmentFiles.revokeProfile(context, it.name)
                        it.deleteRecursively()
                    }
                    sender.close()
                    session.signOut()
                    AndroidStreamLifecycle.enabled = enabled
                }
            }
        }
}
