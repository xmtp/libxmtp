package org.xmtp.android.library.messages

import java.util.Date

typealias UnsignedPublicKey = org.xmtp.proto.message.contents.PublicKeyOuterClass.UnsignedPublicKey

fun UnsignedPublicKey.generate(): UnsignedPublicKey {
    val unsigned = UnsignedPublicKey.newBuilder()
    val key = PrivateKey.newBuilder().build().generate()
    val createdNs = (Date().time * 1_000_000)
    unsigned.secp256K1UncompressedBuilder.bytes = key.publicKey.secp256K1Uncompressed.bytes
    unsigned.createdNs = createdNs.toLong()
    return unsigned.build()
}

class UnsignedPublicKeyBuilder {
    companion object {
        fun buildFromPublicKey(publicKey: PublicKey): UnsignedPublicKey {
            return UnsignedPublicKey.newBuilder().apply {
                createdNs = publicKey.timestamp
                secp256K1UncompressedBuilder.bytes = publicKey.secp256K1Uncompressed.bytes
            }.build()
        }
    }
}
