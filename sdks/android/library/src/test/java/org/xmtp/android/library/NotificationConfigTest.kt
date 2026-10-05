package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.xmtp_sdk.ConsentState
import uniffi.xmtp_sdk.NotificationChannel
import uniffi.xmtp_sdk.NotificationConfig

class NotificationConfigTest {
    @Test
    fun preservesProviderTokensAndRulesAtTheBindingBoundary() {
        val channels = listOf(NotificationChannel.Apns("ab".repeat(32)), NotificationChannel.Fcm("fcm:token-_123"))
        for (channel in channels) {
            val config =
                NotificationConfig(
                    channel = channel,
                    consentStates = listOf(ConsentState.DENIED, ConsentState.UNKNOWN),
                    includeWelcomes = false,
                    includeSyncGroups = true,
                    includeCommits = true,
                )
            when (channel) {
                is NotificationChannel.Apns -> assertEquals(NotificationChannel.Apns(channel.token), config.channel)
                is NotificationChannel.Fcm -> assertEquals(NotificationChannel.Fcm(channel.token), config.channel)
                is NotificationChannel.Http -> error("Expected a provider channel")
            }
            assertEquals(listOf(ConsentState.DENIED, ConsentState.UNKNOWN), config.consentStates)
            assertEquals(false, config.includeWelcomes)
            assertEquals(true, config.includeSyncGroups)
            assertEquals(true, config.includeCommits)

            val defaults = NotificationConfig(channel)
            assertEquals(config.channel, defaults.channel)
            assertEquals(null, defaults.consentStates)
            assertEquals(null, defaults.includeWelcomes)
            assertEquals(null, defaults.includeSyncGroups)
            assertEquals(null, defaults.includeCommits)
        }
    }
}
