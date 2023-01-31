package org.xmtp.android.library

import android.util.Base64
import com.google.crypto.tink.subtle.Base64.encodeToString
import org.xmtp.android.library.messages.AuthDataBuilder
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundle
import org.xmtp.android.library.messages.PrivateKeyBundleV1
import org.xmtp.android.library.messages.PublicKey
import org.xmtp.android.library.messages.Token
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.contents.PrivateKeyOuterClass

data class AuthorizedIdentity(
    var address: String,
    var authorized: PublicKey,
    var identity: PrivateKey,
) {

    constructor(privateKeyBundleV1: PrivateKeyBundleV1) : this(
        privateKeyBundleV1.identityKey.walletAddress,
        privateKeyBundleV1.identityKey.publicKey,
        privateKeyBundleV1.identityKey,
    )

    fun createAuthToken(): String {
        val authData = AuthDataBuilder.buildFromWalletAddress(walletAddress = address)
        val signature = PrivateKeyBuilder(identity).sign(Util.keccak256(authData.toByteArray()))

        val token = Token.newBuilder().also {
            it.identityKey = authorized
            it.authDataBytes = authData.toByteString()
            it.authDataSignature = signature
        }.build().toByteArray()
        return encodeToString(token, Base64.NO_WRAP)
    }

    val toBundle: PrivateKeyBundle
        get() {
            return PrivateKeyOuterClass.PrivateKeyBundle.newBuilder().also {
                it.v1Builder.also { v1Builder ->
                    v1Builder.identityKey = identity
                    v1Builder.identityKeyBuilder.publicKey = authorized
                }.build()
            }.build()
        }
}
