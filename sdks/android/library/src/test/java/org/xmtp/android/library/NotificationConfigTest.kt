package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.xmtpv3.FfiConsentState
import uniffi.xmtpv3.FfiNotificationChannel

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
                ).toFfi()
            when (channel) {
                is NotificationChannel.Apns -> assertEquals(FfiNotificationChannel.Apns(channel.token), config.channel)
                is NotificationChannel.Fcm -> assertEquals(FfiNotificationChannel.Fcm(channel.token), config.channel)
                is NotificationChannel.Http -> error("Expected a provider channel")
            }
            assertEquals(listOf(FfiConsentState.DENIED, FfiConsentState.UNKNOWN), config.consentStates)
            assertEquals(false, config.includeWelcomes)
            assertEquals(true, config.includeSyncGroups)
            assertEquals(true, config.includeCommits)

            val defaults = NotificationConfig(channel).toFfi()
            assertEquals(config.channel, defaults.channel)
            assertEquals(listOf(FfiConsentState.ALLOWED), defaults.consentStates)
            assertEquals(true, defaults.includeWelcomes)
            assertEquals(false, defaults.includeSyncGroups)
            assertEquals(false, defaults.includeCommits)
        }
    }
}
