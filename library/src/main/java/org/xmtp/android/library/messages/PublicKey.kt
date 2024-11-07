package org.xmtp.android.library.messages

import org.bouncycastle.util.Arrays
import org.web3j.crypto.Keys
import org.xmtp.android.library.toHex

typealias PublicKey = org.xmtp.proto.message.contents.PublicKeyOuterClass.PublicKey
val PublicKey.walletAddress: String
    get() {
        val address = Keys.getAddress(
            Arrays.copyOfRange(
                secp256K1Uncompressed.bytes.toByteArray(),
                1,
                secp256K1Uncompressed.bytes.toByteArray().size
            )
        )
        return Keys.toChecksumAddress(address.toHex())
    }
