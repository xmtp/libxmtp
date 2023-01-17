package org.xmtp.android.library.messages

import org.xmtp.proto.message.api.v1.Authn
import java.util.Date

typealias AuthData = org.xmtp.proto.message.api.v1.Authn.AuthData

class AuthDataBuilder {
    companion object {
        fun buildFromWalletAddress(walletAddress: String, timestamp: Date? = null): Authn.AuthData {
            val timestamped = timestamp?.time ?: System.currentTimeMillis()
            return AuthData.newBuilder().apply {
                walletAddr = walletAddress
                createdNs = timestamped * 1_000_000
            }.build()
        }
    }
}
