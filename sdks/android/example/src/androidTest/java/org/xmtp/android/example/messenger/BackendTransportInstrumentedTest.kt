package org.xmtp.android.example.messenger

import android.security.NetworkSecurityPolicy
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import uniffi.xmtp_sdk.*
import java.util.UUID

class BackendTransportInstrumentedTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext

    @Test fun effectiveCleartextPolicyAllowsOnlyLiteralDevelopmentHosts() {
        val policy = NetworkSecurityPolicy.getInstance()
        for (host in listOf("localhost", "127.0.0.1", "::1", "[::1]", "10.0.2.2")) {
            assertTrue("Local cleartext: $host", policy.isCleartextTrafficPermitted(host))
        }
        for (host in listOf(
            "example.com",
            "192.168.1.2",
            "localhost.example.com",
            "sub.localhost",
            "127.0.0.1.example.com",
            "127.1",
            "2130706433",
            "::ffff:127.0.0.1",
        )) {
            assertFalse("Remote cleartext: $host", policy.isCleartextTrafficPermitted(host))
        }
        assertFalse(policy.isCleartextTrafficPermitted)
        println("TRANSPORT_PROOF stage=effective-platform-policy-local-only")
    }

    @Test fun remoteHttpConnectAndSavedRestoreStopBeforeSdkAndLocalRestoreWorks() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = AppSession(context)
            var local: BackendProfile? = null
            var remote: BackendProfile? = null
            var recreated: AppSession? = null
            var sdkStarts = 0
            var originalProfileIds = emptySet<String>()
            try {
                session.signOut()
                originalProfileIds =
                    session.preferences
                        .profiles()
                        .map { it.id }
                        .toSet()
                val localUrl = validatedBackendUrl(BuildConfig.XMTP_BACKEND_URL)
                assertTrue(localAttachmentNetwork(localUrl))
                session.beforeClientBuild = { url ->
                    sdkStarts += 1
                    println("TRANSPORT_PROOF stage=sdk-construction-boundary url=$url")
                    check(url == localUrl) { "Remote HTTP reached the SDK boundary" }
                }
                session.connect(localUrl, "", localAttachmentNetwork(localUrl))
                val owner = checkNotNull(session.active.value)
                local = owner.profile
                val inbox = owner.client.inboxId()
                val groupId =
                    owner.client.conversations
                        .createGroup(
                            emptyList(),
                            CreateGroupOptions(name = "Local transport"),
                        ).id()
                assertEquals(1, sdkStarts)
                val profiles = session.preferences.profiles()
                val rejected = runCatching { session.connect("http://remote.example.test", "test-only-secret", false) }
                assertTrue(rejected.exceptionOrNull() is IllegalArgumentException)
                assertEquals(1, sdkStarts)
                assertEquals(owner.key, session.active.value?.key)
                assertEquals(profiles, session.preferences.profiles())
                assertTrue(session.preferences.signedIn())
                assertEquals(owner.client.inboxId(), checkNotNull(session.active.value).client.inboxId())
                println("TRANSPORT_PROOF stage=remote-connect-rejected-before-profile-secret-sdk")
                session.signOut()
                session.preferences.setSignedIn(true)
                session.restore()
                assertEquals(2, sdkStarts)
                val restored = checkNotNull(session.active.value)
                assertEquals(inbox, restored.client.inboxId())
                assertTrue(restored.profile.allowPrivateNetwork)
                assertTrue(
                    restored.client.conversations
                        .listGroups(null)
                        .any { it.id() == groupId },
                )
                println("TRANSPORT_PROOF stage=actual-local-native-connect-and-restore")
                session.signOut()
                remote = BackendProfile(UUID.randomUUID().toString(), "http://saved-remote.example.test")
                session.preferences.setActive(checkNotNull(remote))
                session.preferences.setSignedIn(true)
                session.secrets.write(checkNotNull(remote).id, "credential", "saved-test-only-secret".toByteArray())
                recreated = AppSession(context)
                var restoreSdkStarts = 0
                checkNotNull(recreated).beforeClientBuild = {
                    restoreSdkStarts += 1
                    error("Saved remote HTTP reached the SDK boundary")
                }
                val restore = runCatching { checkNotNull(recreated).restore() }
                assertTrue(restore.exceptionOrNull() is IllegalArgumentException)
                assertEquals(0, restoreSdkStarts)
                assertNull(checkNotNull(recreated).active.value)
                assertArrayEquals(
                    "saved-test-only-secret".toByteArray(),
                    session.secrets.read(checkNotNull(remote).id, "credential"),
                )
                println("TRANSPORT_PROOF stage=saved-remote-credential-rejected-before-sdk")
            } finally {
                withContext(NonCancellable) {
                    session.beforeClientBuild = {}
                    recreated?.beforeClientBuild = {}
                    recreated?.signOut()
                    session.signOut()
                    local?.let {
                        session.preferences.setActive(it)
                        session.deleteAccount()
                    }
                    remote?.let {
                        session.secrets.delete(it.id, "credential")
                        session.preferences.removeProfile(it.id)
                        it.paths(context.filesDir).root.deleteRecursively()
                    }
                    session.preferences
                        .profiles()
                        .filter {
                            it.id !in originalProfileIds && it.backend == "http://remote.example.test"
                        }.forEach {
                            for (ref in listOf("credential", "wallet", "database-key")) {
                                session.secrets.delete(it.id, ref)
                            }
                            session.preferences.removeProfile(it.id)
                            it.paths(context.filesDir).root.deleteRecursively()
                        }
                }
                AndroidStreamLifecycle.enabled = true
            }
        }
}
