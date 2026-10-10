package org.xmtp.android.example.messenger

import android.content.pm.ApplicationInfo
import android.security.NetworkSecurityPolicy
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.MessengerAction
import uniffi.xmtp_sdk.*
import java.net.URI
import java.util.UUID

class ReleaseTransportInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private fun assertRelease() {
        assertFalse(BuildConfig.DEBUG)
        assertEquals(0, compose.activity.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE)
    }

    @Test fun releaseBridgeConnectRestoreAndProbeStopBeforeSdk() =
        runBlocking {
            assertRelease()
            val session = model.session
            val originalProfiles =
                session.preferences
                    .profiles()
                    .map { it.id }
                    .toSet()
            try {
                session.signOut()
                var sdkStarts = 0
                session.beforeClientBuild = { url ->
                    sdkStarts += 1
                    println("RELEASE_TRANSPORT_PROOF stage=unexpected-sdk-boundary url=$url")
                    error("Rejected HTTP entered SDK construction")
                }
                for (url in listOf("http://10.0.2.2:5050", "http://example.com")) {
                    val before = session.preferences.profiles()
                    val result = runCatching { session.connect(url, "test-only-secret", false) }
                    assertTrue(url, result.exceptionOrNull() is IllegalArgumentException)
                    assertEquals(0, sdkStarts)
                    assertEquals(before, session.preferences.profiles())
                    assertNull(session.active.value)
                    assertFalse(localAttachmentNetwork(url))
                }
                val saved = BackendProfile(UUID.randomUUID().toString(), "http://10.0.2.2:5050")
                session.preferences.setActive(saved)
                session.preferences.setSignedIn(true)
                session.secrets.write(saved.id, "credential", "saved-test-only-secret".toByteArray())
                val restore = runCatching { session.restore() }
                assertTrue(restore.exceptionOrNull() is IllegalArgumentException)
                assertEquals(0, sdkStarts)
                assertNull(session.active.value)
                val url = "http://10.0.2.2:5050"
                val finished = CompletableDeferred<Unit>()
                var probes = 0
                model.inspectBackend = {
                    probes += 1
                    error("Rejected release bridge entered the SDK probe")
                }
                model.onBackendProbeFinished = { _, current -> if (current == url) finished.complete(Unit) }
                model.dispatch(MessengerAction.InspectBackend(url))
                withTimeout(30_000) { finished.await() }
                assertEquals(0, probes)
                assertNull(model.state.value.credentialsRequiredFor)
                println("RELEASE_TRANSPORT_PROOF stage=bridge-remote-connect-saved-credential-and-probe-rejected")
            } finally {
                withContext(NonCancellable) {
                    session.beforeClientBuild = {}
                    model.inspectBackend =
                        { SDKClient.fetchServerConfiguration(BackendSource.Options(BackendOptions(url = it))) }
                    model.onBackendProbeFinished = { _, _ -> }
                    session.signOut()
                    session.preferences.profiles().filter { it.id !in originalProfiles }.forEach {
                        for (ref in listOf("credential", "wallet", "database-key")) session.secrets.delete(it.id, ref)
                        session.preferences.removeProfile(it.id)
                        it.paths(compose.activity.filesDir).root.deleteRecursively()
                    }
                }
            }
        }

    @Test fun releaseApkPolicyAllowsRealLoopbackSdkAndRejectsTheBridge() =
        runBlocking {
            assertRelease()
            val policy = NetworkSecurityPolicy.getInstance()
            assertFalse("Release bridge cleartext", policy.isCleartextTrafficPermitted("10.0.2.2"))
            assertFalse("Release remote cleartext", policy.isCleartextTrafficPermitted("example.com"))
            assertFalse(policy.isCleartextTrafficPermitted)
            for (host in listOf("localhost", "127.0.0.1", "::1", "[::1]")) {
                assertTrue(host, policy.isCleartextTrafficPermitted(host))
            }
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = model.session
            try {
                session.signOut()
                val port = URI(BuildConfig.XMTP_BACKEND_URL).port
                assertTrue(port > 0)
                val url = "http://127.0.0.1:$port"
                assertEquals(url, validatedBackendUrl(url))
                session.connect(url, "", localAttachmentNetwork(url))
                val owner = checkNotNull(session.active.value)
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Release loopback"),
                    )
                assertEquals("Release loopback", group.state().name)
                assertTrue(owner.profile.allowPrivateNetwork)
                println("RELEASE_TRANSPORT_PROOF stage=effective-release-policy-and-real-loopback-native-sdk")
            } finally {
                withContext(NonCancellable) {
                    if (session.active.value != null) session.deleteAccount()
                    session.signOut()
                }
                AndroidStreamLifecycle.enabled = true
            }
        }
}
