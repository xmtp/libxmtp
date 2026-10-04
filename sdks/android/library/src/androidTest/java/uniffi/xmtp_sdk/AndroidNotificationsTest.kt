package uniffi.xmtp_sdk

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.xmtp.android.library.BuildConfig
import java.security.SecureRandom
import java.util.UUID

class AndroidNotificationsTest {
    private var previousLifecycle = true

    @Before fun setup() =
        runBlocking {
            previousLifecycle = AndroidStreamLifecycle.enabled
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
        }

    @After fun teardown() {
        AndroidStreamLifecycle.enabled = previousLifecycle
    }

    @Test fun registersHttpAndResetsGroupAndDmOverrides() =
        runBlocking {
            val context = InstrumentationRegistry.getInstrumentation().targetContext

            fun options() =
                ClientOptions(
                    backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                    storage =
                        StorageOptions(
                            location = StorageLocation.Default,
                            label = "notification-rules-${UUID.randomUUID()}",
                        ),
                )
            val client = SDKClient.create(context, generateLocalSigner(), options())
            try {
                val peer = SDKClient.create(context, generateLocalSigner(), options())
                try {
                    val group = client.conversations().createGroup(listOf(peer.inboxId()))
                    val dm = client.conversations().createDm(peer.inboxId())
                    val conversations = listOf(Conversation.Group(group), Conversation.Dm(dm))

                    suspend fun enabled(conversation: Conversation) =
                        when (conversation) {
                            is Conversation.Group -> {
                                conversation.group
                                    .state()
                                    .common.notificationsEnabled
                            }

                            is Conversation.Dm -> {
                                conversation.dm.state().notificationsEnabled
                            }
                        }
                    val channel =
                        NotificationChannel.Http(
                            "https://example.com/xmtp-notification-test",
                            SecureRandom().generateSeed(32),
                        )
                    assertEquals(NotificationState.Disabled, client.notificationState())
                    client.enableNotifications(NotificationConfig(channel))
                    assertEquals(NotificationState.Enabled, client.notificationState())
                    for (conversation in conversations) {
                        assertTrue(enabled(conversation))
                        conversation.setNotifications(NotificationOverride.DISABLED)
                        assertFalse(enabled(conversation))
                        conversation.setNotifications(NotificationOverride.DEFAULT)
                        assertTrue(enabled(conversation))
                    }
                    client.enableNotifications(
                        NotificationConfig(
                            channel,
                            consentStates = emptyList(),
                            includeWelcomes = false,
                            includeSyncGroups = false,
                            includeCommits = true,
                        ),
                    )
                    for (conversation in conversations) {
                        assertFalse(enabled(conversation))
                        conversation.setNotifications(NotificationOverride.ENABLED)
                        assertTrue(enabled(conversation))
                        conversation.setNotifications(NotificationOverride.DEFAULT)
                        assertFalse(enabled(conversation))
                    }
                    client.disableNotifications()
                    assertEquals(NotificationState.Disabled, client.notificationState())
                } finally {
                    withContext(NonCancellable) { peer.storage().delete() }
                }
            } finally {
                withContext(NonCancellable) { client.storage().delete() }
            }
        }

    @Test fun restoresNotificationRulesAndOverridesAfterRestart() =
        runBlocking {
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val signer = generateLocalSigner()
            val options =
                ClientOptions(
                    backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                    storage =
                        StorageOptions(
                            location = StorageLocation.Default,
                            label = "notifications-${UUID.randomUUID()}",
                        ),
                )
            var client = SDKClient.create(context, signer, options)
            try {
                val group = client.conversations().createGroup(emptyList<InboxId>())
                val groupId = group.id()
                val path = client.storage().path()
                val installation = client.installationId()
                client.enableNotifications(
                    NotificationConfig(
                        NotificationChannel.Http(
                            "https://example.com/xmtp-notification-test",
                            SecureRandom().generateSeed(32),
                        ),
                        consentStates = emptyList(),
                        includeWelcomes = false,
                    ),
                )
                group.setNotifications(NotificationOverride.ENABLED)
                client.end()
                client = SDKClient.build(context, signer.identity(), options)
                assertEquals(path, client.storage().path())
                assertEquals(installation, client.installationId())
                assertEquals(NotificationState.Enabled, client.notificationState())
                val restored = (checkNotNull(client.conversations().getById(groupId)) as Conversation.Group).group
                assertTrue(restored.state().common.notificationsEnabled)
                restored.setNotifications(NotificationOverride.DEFAULT)
                assertFalse(restored.state().common.notificationsEnabled)
                client.disableNotifications()
                client.end()
                client = SDKClient.build(context, signer.identity(), options)
                assertEquals(installation, client.installationId())
                assertEquals(NotificationState.Disabled, client.notificationState())
                for (channel in listOf(NotificationChannel.Apns("a".repeat(64)), NotificationChannel.Fcm("token"))) {
                    try {
                        client.enableNotifications(NotificationConfig(channel))
                        fail("Expected channel failure")
                    } catch (
                        error: XmtpException.ChannelNotConfigured,
                    ) {
                        val state = client.notificationState()
                        assertTrue(state is NotificationState.Failed)
                        assertEquals(
                            NotificationFailure.CHANNEL_NOT_CONFIGURED,
                            (state as NotificationState.Failed).error,
                        )
                    }
                    client.disableNotifications()
                    assertEquals(NotificationState.Disabled, client.notificationState())
                }
            } finally {
                withContext(NonCancellable) { client.storage().delete() }
            }
        }
}
