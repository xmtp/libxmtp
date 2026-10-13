package org.xmtp.android.example.messenger.data.xmtp

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.messenger.feature.FeatureInputFailure
import uniffi.xmtp_sdk.PublicIdentity
import uniffi.xmtp_sdk.PublicIdentityKind

class RecipientResolverTest {
    private val inbox = "a1".repeat(32)

    @Test fun handlesUseTheVerifiedHandleLookupIncludingShortAndCodedNames() =
        runBlocking<Unit> {
            for (input in listOf(" Nick ", "Nick#2341", "@Nick", "@Nick#2341", "n")) {
                val lookedUp = mutableListOf<PublicIdentity>()
                assertEquals(
                    inbox,
                    resolveRecipient(input) {
                        lookedUp += it
                        inbox
                    },
                )
                assertEquals(
                    listOf(PublicIdentity(input.trim().removePrefix("@").lowercase(), PublicIdentityKind.HANDLE)),
                    lookedUp,
                )
            }
        }

    @Test fun ethereumPrefixSelectsEthereumEvenWhenTheRestIsMalformed() =
        runBlocking<Unit> {
            val address = "0X" + "Ab".repeat(20)
            val lookedUp = mutableListOf<PublicIdentity>()
            assertEquals(
                inbox,
                resolveRecipient(" " + address + " ") {
                    lookedUp += it
                    inbox
                },
            )
            assertEquals(listOf(PublicIdentity(address.lowercase(), PublicIdentityKind.ETHEREUM)), lookedUp)
            for (input in listOf("0xnick", "0x" + "a".repeat(64), "0x")) {
                assertThrows(FeatureInputFailure::class.java) {
                    runBlocking<Unit> { resolveRecipient(input) { error("Malformed address reached the network") } }
                }
            }
        }

    @Test fun inboxIdsAreCanonicalAndNeedNoIdentityLookup() =
        runBlocking<Unit> {
            assertEquals(
                inbox,
                resolveRecipient(" " + inbox.uppercase() + " ") { error("Inbox ID reached the network") },
            )
            assertThrows(FeatureInputFailure::class.java) {
                runBlocking<Unit> { resolveRecipient("z".repeat(64)) { error("Invalid inbox reached the network") } }
            }
        }

    @Test fun unknownHandlesAreAnInputErrorAndNeverFallBackToAnInboxId() =
        runBlocking<Unit> {
            val failure =
                assertThrows(FeatureInputFailure::class.java) {
                    runBlocking<Unit> { resolveRecipient("nobody#2341") { null } }
                }
            assertEquals("No XMTP account has this handle.", failure.userMessage)
        }

    @Test fun malformedHandlesAreRejectedBeforeLookup() =
        runBlocking<Unit> {
            for (input in listOf(
                "",
                "@",
                "@@nick",
                "@0xnick",
                "@" + "a".repeat(64),
                "nick.eth",
                "ni ck",
                "nick#12",
                "n".repeat(28),
                "1nick",
            )) {
                assertThrows(FeatureInputFailure::class.java) {
                    runBlocking<Unit> { resolveRecipient(input) { error("Malformed handle reached the network") } }
                }
            }
        }

    @Test fun networkFailuresAndCancellationRemainFailures() =
        runBlocking<Unit> {
            val offline = java.io.IOException("Offline")
            val cancelled = CancellationException("Session changed")
            for (failure in listOf(offline, cancelled)) {
                try {
                    resolveRecipient("nick") { throw failure }
                    fail("Lookup failure was swallowed")
                } catch (actual: Throwable) {
                    assertSame(failure, actual)
                }
            }
        }
}
