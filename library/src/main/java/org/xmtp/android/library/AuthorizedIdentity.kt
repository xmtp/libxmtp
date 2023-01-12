package org.xmtp.android.library

import android.util.Base64
import com.google.crypto.tink.subtle.Base64.*
import org.xmtp.android.library.messages.*
import org.xmtp.proto.message.contents.PrivateKeyOuterClass

data class AuthorizedIdentity(
    var address: String,
    var authorized: PublicKey,
    var identity: PrivateKey
) {

    fun createAuthToken(): String {
        val authData = AuthDataBuilder.buildFromWalletAddress(walletAddress = address)
        val signature = PrivateKeyBuilder(identity).sign(Util.keccak256(authData.toByteArray()))
        authorized.toBuilder().apply {
            this.signature = signature
        }.build()
        val token = Token.newBuilder().apply {
            identityKey = authorized
            authDataBytes = authData.toByteString()
            authDataSignature = signature
        }.build().toByteArray()
        return encodeToString(token, Base64.DEFAULT)
    }

    val toBundle: PrivateKeyBundle
        get() {
            return PrivateKeyOuterClass.PrivateKeyBundle.newBuilder().apply {
                v1Builder.apply {
                    identityKey = identity
                    identityKeyBuilder.apply {
                        publicKey = authorized
                    }.build()
                }.build()
            }.build()
        }
}
