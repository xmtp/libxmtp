package org.xmtp.kotlin

data class SignedData(
    val rawData: ByteArray,
    val publicKey: ByteArray? = null, // Used for Passkeys
    val authenticatorData: ByteArray? = null, // WebAuthn metadata
    val clientDataJson: ByteArray? = null, // WebAuthn metadata
)
