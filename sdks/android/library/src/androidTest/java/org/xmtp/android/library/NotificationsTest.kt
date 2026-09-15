package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import java.security.SecureRandom

class NotificationsTest : BaseInstrumentedTest() {
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
            val group = client.conversations.newGroup(listOf(fixtures.boClient.inboxId))
            val dm = client.conversations.findOrCreateDm(fixtures.boClient.inboxId)
            assertEquals(NotificationState.Disabled, client.notificationState())
            assertEquals(NotificationState.Enabled, client.enableNotifications(httpConfig()))
            assertEquals(NotificationState.Enabled, client.notificationState())

            assertTrue(group.notificationsEnabled())
            group.setNotifications(NotificationOverride.Disabled)
            assertFalse(group.notificationsEnabled())
            group.setNotifications(NotificationOverride.Default)
            assertTrue(group.notificationsEnabled())
            assertTrue(dm.notificationsEnabled())
            dm.setNotifications(NotificationOverride.Disabled)
            assertFalse(dm.notificationsEnabled())
            dm.setNotifications(NotificationOverride.Default)
            assertTrue(dm.notificationsEnabled())

            client.enableNotifications(
                NotificationConfig(
                    channel = httpConfig().channel,
                    consentStates = emptyList(),
                    includeWelcomes = false,
                    includeSyncGroups = false,
                    includeCommits = true,
                    metadata = byteArrayOf(0, -128, -1),
                ),
            )
            for (conversation in listOf(Conversation.Group(group), Conversation.Dm(dm))) {
                assertFalse(conversation.notificationsEnabled())
                conversation.setNotifications(NotificationOverride.Enabled)
                assertTrue(conversation.notificationsEnabled())
                conversation.setNotifications(NotificationOverride.Default)
                assertFalse(conversation.notificationsEnabled())
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
                } catch (error: NotificationError) {
                    assertEquals("NotificationError::ChannelNotConfigured", error.code)
                }
                val state = client.notificationState()
                assertTrue(state is NotificationState.Failed)
                assertEquals("NotificationError::ChannelNotConfigured", (state as NotificationState.Failed).error.code)
                client.disableNotifications()
                assertEquals(NotificationState.Disabled, client.notificationState())
            }
        }
}
