package org.xmtp.android.example.messenger.data.xmtp

import org.xmtp.android.example.messenger.feature.FeatureInputFailure
import org.xmtp.android.example.shared.feature.handles.handleInputProblem
import org.xmtp.android.example.shared.feature.handles.normalizedHandleInput
import uniffi.xmtp_sdk.PublicIdentity
import uniffi.xmtp_sdk.PublicIdentityKind
import uniffi.xmtp_sdk.XmtpException

/** Resolves user input to an inbox before a conversation or membership write. */
internal suspend fun resolveRecipient(
    input: String,
    lookup: suspend (PublicIdentity) -> String?,
): String {
    val value = input.trim()
    if (value.isEmpty()) throw FeatureInputFailure("Enter a handle, Ethereum address, or inbox ID.")
    val identity =
        when {
            value.startsWith("0x", ignoreCase = true) -> {
                if (!value.matches(Regex("0[xX][0-9a-fA-F]{40}"))) {
                    throw FeatureInputFailure(
                        "Enter an Ethereum address with 0x followed by 40 hexadecimal characters.",
                    )
                }
                PublicIdentity(value.lowercase(), PublicIdentityKind.ETHEREUM)
            }

            value.length == 64 -> {
                if (!value.matches(Regex("[0-9a-fA-F]{64}"))) {
                    throw FeatureInputFailure("An inbox ID must contain 64 hexadecimal characters.")
                }
                return value.lowercase()
            }

            else -> {
                val handle = normalizedHandleInput(value)
                // Existing handles remain valid lookup targets if claim policy later changes.
                if (handleInputProblem(handle, minNameLength = 1) != null) {
                    throw FeatureInputFailure(
                        "Enter a handle such as nick or nick#2341, an Ethereum address, or an inbox ID.",
                    )
                }
                PublicIdentity(handle, PublicIdentityKind.HANDLE)
            }
        }
    val inbox =
        try {
            lookup(identity)
        } catch (_: XmtpException.IdentityNotFound) {
            null
        }
    return inbox ?: throw FeatureInputFailure(
        if (identity.kind == PublicIdentityKind.HANDLE) {
            "No XMTP account has this handle."
        } else {
            "This address has no XMTP account."
        },
    )
}
