package uniffi.xmtp_sdk

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// Rust owns the vectors: xmtp_sdk/src/signer.rs::pure_inbox_calculation_matches_fixed_core_vectors,
// ::pure_inbox_calculation_keeps_identity_validation_and_full_nonce.
// This checks the generated ULong lowering of the nonce and the typed error.
class PureInboxIdTest {
    // The core vectors cross the native boundary with no client, including the full ULong nonce.
    @Test
    fun pureInboxIdKeepsTheFullNonce() {
        val ethereum = PublicIdentity("0xabcdef0000000000000000000000000000000000", PublicIdentityKind.ETHEREUM)
        val expected =
            mapOf(
                0uL to "139a684d70154ab320b846179e5219b6e2d192048577779b230763a85a28365d",
                9_007_199_254_740_993uL to "7388e86684247cde39d20ef985b6999cb657325d1576c28b74670a5392228913",
                ULong.MAX_VALUE to "00b23df9cd0b16c488b19647e02ee5872a8c7de14d056b937d1cd1f54c4a28fc",
            )
        assertEquals(expected.getValue(0uL), generateInboxId(ethereum))
        for ((nonce, inboxId) in expected) assertEquals("nonce $nonce", inboxId, generateInboxId(ethereum, nonce))
        val invalid =
            runCatching {
                generateInboxId(
                    PublicIdentity("invalid", PublicIdentityKind.ETHEREUM),
                )
            }.exceptionOrNull()
        assertTrue("Expected InvalidArgument, got $invalid", invalid is XmtpException.InvalidArgument)
        assertEquals(ErrorCategory.INPUT, (invalid as XmtpException.InvalidArgument).v1.category)
    }
}
