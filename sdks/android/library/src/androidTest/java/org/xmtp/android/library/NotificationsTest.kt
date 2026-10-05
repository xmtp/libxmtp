package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import uniffi.xmtp_sdk.*
import java.security.SecureRandom

class NotificationsTest : BaseInstrumentedTest() {
    @Test
    fun restoresNotificationStateRulesAndOverridesAfterRestart() =
        runBlocking {
            val wallet = createWallet()
            val options = createClientOptions(localApi(), deviceSyncEnabled = true)
            var client = trackClient(SDKClient.create(context, wallet, options))
            try {
                val group = client.conversations().createGroup(emptyList<InboxId>())
                val groupId = group.id()
                val installationId = client.installationId()
                val dbPath = client.storagePath()
                client.enableNotifications(
                    NotificationConfig(
                        channel = httpConfig().channel,
                        consentStates = emptyList(),
                        includeWelcomes = false,
                    ),
                )
                group.setNotifications(NotificationOverride.ENABLED)
                client.end()
                client = trackClient(SDKClient.create(context, wallet, options))
                assertEquals(dbPath, client.storagePath())
                assertEquals(installationId, client.installationId())
                assertEquals(NotificationState.Enabled, client.notificationState())
                val restored = (checkNotNull(client.conversations().getById(groupId)) as Conversation.Group).group
                assertTrue(restored.state().common.notificationsEnabled)
                restored.setNotifications(NotificationOverride.DEFAULT)
                assertFalse(restored.state().common.notificationsEnabled)
                client.disableNotifications()
                client.end()
                client = trackClient(SDKClient.create(context, wallet, options))
                assertEquals(installationId, client.installationId())
                assertEquals(NotificationState.Disabled, client.notificationState())
            } finally {
                client.end()
            }
        }

    private fun httpConfig(): NotificationConfig =
        NotificationConfig(
            channel =
                NotificationChannel.Http(
                    "https://example.com/xmtp-notification-test",
                    SecureRandom().generateSeed(32),
                ),
        )

    @Test
    fun registersHttpAndResetsGroupAndDmOverrides() =
        runBlocking {
            val fixtures = createFixtures()
            val client = fixtures.alixClient
            val group = client.conversations().createGroup(listOf(fixtures.boClient.inboxId()))
            val dm = client.conversations().createDm(fixtures.boClient.inboxId())
            assertEquals(NotificationState.Disabled, client.notificationState())
            assertEquals(NotificationState.Enabled, client.enableNotifications(httpConfig()))
            assertEquals(NotificationState.Enabled, client.notificationState())

            assertTrue(group.state().common.notificationsEnabled)
            group.setNotifications(NotificationOverride.DISABLED)
            assertFalse(group.state().common.notificationsEnabled)
            group.setNotifications(NotificationOverride.DEFAULT)
            assertTrue(group.state().common.notificationsEnabled)
            assertTrue(dm.state().notificationsEnabled)
            dm.setNotifications(NotificationOverride.DISABLED)
            assertFalse(dm.state().notificationsEnabled)
            dm.setNotifications(NotificationOverride.DEFAULT)
            assertTrue(dm.state().notificationsEnabled)

            client.enableNotifications(
                NotificationConfig(
                    channel = httpConfig().channel,
                    consentStates = emptyList(),
                    includeWelcomes = false,
                    includeSyncGroups = false,
                    includeCommits = true,
                ),
            )
            for (conversation in listOf(Conversation.Group(group), Conversation.Dm(dm))) {
                assertFalse(notificationEnabled(conversation))
                conversation.setNotifications(NotificationOverride.ENABLED)
                assertTrue(notificationEnabled(conversation))
                conversation.setNotifications(NotificationOverride.DEFAULT)
                assertFalse(notificationEnabled(conversation))
            }
            client.disableNotifications()
            assertEquals(NotificationState.Disabled, client.notificationState())
        }

    @Test
    fun returnsTypedFailuresForUnconfiguredChannels() =
        runBlocking {
            val client = createClient(createWallet())
            for (channel in listOf(NotificationChannel.Apns("a".repeat(64)), NotificationChannel.Fcm("token"))) {
                try {
                    client.enableNotifications(NotificationConfig(channel))
                    fail("An unconfigured channel must fail")
                } catch (error: XmtpException.ChannelNotConfigured) {
                    assertEquals("ChannelNotConfigured", error.v1.code)
                    assertEquals(ErrorCategory.NOTIFICATION, error.v1.category)
                    assertFalse(error.v1.retryable)
                }
                val state = client.notificationState()
                assertTrue(state is NotificationState.Failed)
                assertEquals(NotificationFailure.CHANNEL_NOT_CONFIGURED, (state as NotificationState.Failed).error)
                client.disableNotifications()
                assertEquals(NotificationState.Disabled, client.notificationState())
            }
        }

    private suspend fun notificationEnabled(conversation: Conversation): Boolean =
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
}
