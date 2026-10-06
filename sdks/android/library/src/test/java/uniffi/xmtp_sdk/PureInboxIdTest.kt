package uniffi.xmtp_sdk

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// Rust owns the vectors: xmtp_sdk/src/signer.rs::pure_inbox_calculation_matches_fixed_core_vectors,
// ::pure_inbox_calculation_keeps_identity_validation_and_full_nonce.
// This checks the generated lowering of each PublicIdentityKind and of the
// ULong nonce, and the typed error lift. The vectors are the ones the deleted
// Kotlin conformance check (RetainedInboxId.kt) used.
class PureInboxIdTest {
    private val nonces = listOf(0uL, 1uL, 9_007_199_254_740_993uL, ULong.MAX_VALUE)

    private val vectors =
        listOf(
            PublicIdentity("0xabcdef0000000000000000000000000000000000", PublicIdentityKind.ETHEREUM) to
                listOf(
                    "139a684d70154ab320b846179e5219b6e2d192048577779b230763a85a28365d",
                    "f020cf771dabaf2610250b5f00076215a8f1da8649ba46cf5ba2d00df6ce5279",
                    "7388e86684247cde39d20ef985b6999cb657325d1576c28b74670a5392228913",
                    "00b23df9cd0b16c488b19647e02ee5872a8c7de14d056b937d1cd1f54c4a28fc",
                ),
            PublicIdentity("abcdef", PublicIdentityKind.PASSKEY) to
                listOf(
                    "e26bbe40a904acb658e0dd48f4031811b662ce4e6238eef5c46f5bb92550713a",
                    "ac9f830ae6cf2299ba293dd4cec3be0d87a88e6a8fbfe5015de6fffd11d79b6e",
                    "ef91da01728a1d16593d300a7a699d6a7831c43e00916a6b199f8244f207637d",
                    "469fa9bb87114e117a27305350728cb2a4f85fdae9b5ccaef3b702f879dd9ae4",
                ),
        )

    // The core vectors cross the native boundary with no client, for each
    // identity kind, with an omitted, absent and full ULong nonce.
    @Test
    fun pureInboxIdKeepsEachKindAndTheFullNonce() {
        assertEquals(PublicIdentityKind.entries.toSet(), vectors.map { it.first.kind }.toSet())
        for ((identity, expected) in vectors) {
            val label = identity.kind
            assertEquals("$label omitted nonce", expected[0], generateInboxId(identity))
            assertEquals("$label absent nonce", expected[0], generateInboxId(identity, null))
            for ((nonce, inboxId) in nonces.zip(expected)) {
                assertEquals("$label nonce $nonce", inboxId, generateInboxId(identity, nonce))
            }
        }
    }

    @Test
    fun invalidIdentityOfEachKindIsInvalidArgument() {
        val invalid =
            listOf(
                PublicIdentity("invalid", PublicIdentityKind.ETHEREUM),
                PublicIdentity("not hex", PublicIdentityKind.PASSKEY),
            )
        for (identity in invalid) {
            val failure = runCatching { generateInboxId(identity) }.exceptionOrNull()
            assertTrue(
                "${identity.kind}: expected InvalidArgument, got $failure",
                failure is XmtpException.InvalidArgument,
            )
            val details = (failure as XmtpException.InvalidArgument).v1
            assertEquals(ErrorCategory.INPUT, details.category)
            assertFalse(details.retryable)
        }
    }
}
