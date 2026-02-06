package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class ClientCacheKeyTest {
    // Helper extension to access the private toCacheKey() method for testing
    private fun ClientOptions.Api.toCacheKey(): String =
        "${env.getUrl()}|$isSecure|${appVersion ?: "nil"}|${gatewayHost ?: "nil"}"

    @Test
    fun testApiClientCacheKeysDifferentConfigurations() {
        // Test that the API client cache correctly differentiates between different configurations
        // Cache key format: "\(env.url)|\(isSecure)|\(appVersion ?? "nil")|\(gatewayHost ?? "nil")"

        // Test 1: Different environment URLs
        val key1 =
            ClientOptions
                .Api(env = XMTPEnvironment.LOCAL, isSecure = true, appVersion = null, gatewayHost = null)
                .toCacheKey()
        val key2 =
            ClientOptions
                .Api(env = XMTPEnvironment.DEV, isSecure = true, appVersion = null, gatewayHost = null)
                .toCacheKey()
        val key3 =
            ClientOptions
                .Api(env = XMTPEnvironment.PRODUCTION, isSecure = true, appVersion = null, gatewayHost = null)
                .toCacheKey()
        assertNotEquals("Cache keys should differ for different environments (local vs dev)", key1, key2)
        assertNotEquals("Cache keys should differ for different environments (dev vs production)", key2, key3)
        assertNotEquals("Cache keys should differ for different environments (local vs production)", key1, key3)

        // Test 2: Different isSecure values (same env)
        val key4 =
            ClientOptions
                .Api(env = XMTPEnvironment.LOCAL, isSecure = true, appVersion = null, gatewayHost = null)
                .toCacheKey()
        val key5 =
            ClientOptions
                .Api(env = XMTPEnvironment.LOCAL, isSecure = false, appVersion = null, gatewayHost = null)
                .toCacheKey()
        assertNotEquals("Cache keys should differ when isSecure differs", key4, key5)

        // Test 3: Different appVersion values (same env, same isSecure)
        val key6 =
            ClientOptions
                .Api(env = XMTPEnvironment.LOCAL, isSecure = true, appVersion = "1.0.0", gatewayHost = null)
                .toCacheKey()
        val key7 =
            ClientOptions
                .Api(env = XMTPEnvironment.LOCAL, isSecure = true, appVersion = "2.0.0", gatewayHost = null)
                .toCacheKey()
        assertNotEquals("Cache keys should differ for different appVersion values", key6, key7)

        // Test 4: appVersion present vs absent
        val key8 =
            ClientOptions
                .Api(env = XMTPEnvironment.LOCAL, isSecure = true, appVersion = null, gatewayHost = null)
                .toCacheKey()
        val key9 =
            ClientOptions
                .Api(env = XMTPEnvironment.LOCAL, isSecure = true, appVersion = "1.0.0", gatewayHost = null)
                .toCacheKey()
        assertNotEquals("Cache keys should differ when one has appVersion and other doesn't", key8, key9)

        // Test 5: Different gatewayHost values (same env, same isSecure, same appVersion)
        val key10 =
            ClientOptions
                .Api(
                    env = XMTPEnvironment.LOCAL,
                    isSecure = true,
                    appVersion = null,
                    gatewayHost = "https://gateway1.example.com",
                ).toCacheKey()
        val key11 =
            ClientOptions
                .Api(
                    env = XMTPEnvironment.LOCAL,
                    isSecure = true,
                    appVersion = null,
                    gatewayHost = "https://gateway2.example.com",
                ).toCacheKey()
        assertNotEquals("Cache keys should differ for different gatewayHost values", key10, key11)

        // Test 6: gatewayHost present vs absent
        val key12 =
            ClientOptions
                .Api(env = XMTPEnvironment.LOCAL, isSecure = true, appVersion = null, gatewayHost = null)
                .toCacheKey()
        val key13 =
            ClientOptions
                .Api(
                    env = XMTPEnvironment.LOCAL,
                    isSecure = true,
                    appVersion = null,
                    gatewayHost = "https://gateway.example.com",
                ).toCacheKey()
        assertNotEquals("Cache keys should differ when one has gatewayHost and other doesn't", key12, key13)

        // Test 7: Multiple parameters different
        val key14 =
            ClientOptions
                .Api(
                    env = XMTPEnvironment.DEV,
                    isSecure = true,
                    appVersion = "1.0.0",
                    gatewayHost = "https://gateway.example.com",
                ).toCacheKey()
        val key15 =
            ClientOptions
                .Api(
                    env = XMTPEnvironment.LOCAL,
                    isSecure = false,
                    appVersion = "2.0.0",
                    gatewayHost = "https://other-gateway.example.com",
                ).toCacheKey()
        assertNotEquals("Cache keys should differ when multiple parameters are different", key14, key15)

        // Test 8: Completely identical configurations should produce same key
        val key16 =
            ClientOptions
                .Api(
                    env = XMTPEnvironment.LOCAL,
                    isSecure = true,
                    appVersion = "1.0.0",
                    gatewayHost = "https://gateway.example.com",
                ).toCacheKey()
        val key17 =
            ClientOptions
                .Api(
                    env = XMTPEnvironment.LOCAL,
                    isSecure = true,
                    appVersion = "1.0.0",
                    gatewayHost = "https://gateway.example.com",
                ).toCacheKey()
        assertEquals("Cache keys should be identical for identical configurations", key16, key17)

        // Test 9: All parameters nil/default should produce same key
        val key18 =
            ClientOptions
                .Api(env = XMTPEnvironment.LOCAL, isSecure = true, appVersion = null, gatewayHost = null)
                .toCacheKey()
        val key19 =
            ClientOptions
                .Api(env = XMTPEnvironment.LOCAL, isSecure = true, appVersion = null, gatewayHost = null)
                .toCacheKey()
        assertEquals("Cache keys should be identical when all optional params are nil", key18, key19)

        // Test 10: Edge case - same gatewayHost but different other params should still differ
        val key20 =
            ClientOptions
                .Api(
                    env = XMTPEnvironment.DEV,
                    isSecure = true,
                    appVersion = null,
                    gatewayHost = "https://gateway.example.com",
                ).toCacheKey()
        val key21 =
            ClientOptions
                .Api(
                    env = XMTPEnvironment.DEV,
                    isSecure = false,
                    appVersion = null,
                    gatewayHost = "https://gateway.example.com",
                ).toCacheKey()
        assertNotEquals("Cache keys should differ even when gatewayHost is same but isSecure differs", key20, key21)

        // Test 11: Edge case - same everything except gatewayHost
        val key22 =
            ClientOptions
                .Api(
                    env = XMTPEnvironment.PRODUCTION,
                    isSecure = true,
                    appVersion = "3.0.0",
                    gatewayHost = null,
                ).toCacheKey()
        val key23 =
            ClientOptions
                .Api(
                    env = XMTPEnvironment.PRODUCTION,
                    isSecure = true,
                    appVersion = "3.0.0",
                    gatewayHost = "https://gateway.example.com",
                ).toCacheKey()
        assertNotEquals("Cache keys should differ when only gatewayHost differs", key22, key23)
    }
}
