package org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiSignatureRequest

class SignatureRequest(
    val ffiSignatureRequest: FfiSignatureRequest,
) {
    suspend fun addScwSignature(
        signatureBytes: ByteArray,
        address: String,
        chainId: ULong,
        blockNumber: ULong? = null,
    ) {
        ffiSignatureRequest.addScwSignature(signatureBytes, address, chainId, blockNumber)
    }

    suspend fun addEcdsaSignature(signatureBytes: ByteArray) {
        ffiSignatureRequest.addEcdsaSignature(signatureBytes)
    }

    suspend fun signatureText(): String = ffiSignatureRequest.signatureText()
}
