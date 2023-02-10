package org.xmtp.android.library

import com.google.protobuf.kotlin.toByteString
import org.web3j.crypto.ECDSASignature
import org.web3j.crypto.Keys
import org.web3j.crypto.Sign
import org.xmtp.android.library.messages.PublicKey
import org.xmtp.android.library.messages.Signature
import org.xmtp.android.library.messages.createIdentityText
import org.xmtp.android.library.messages.ethHash
import org.xmtp.android.library.messages.rawData
import org.xmtp.proto.message.contents.PrivateKeyOuterClass
import org.xmtp.proto.message.contents.PublicKeyOuterClass
import org.xmtp.proto.message.contents.SignatureOuterClass
import java.math.BigInteger
import java.util.Date

interface SigningKey {
    val address: String

    fun sign(data: ByteArray): SignatureOuterClass.Signature

    fun sign(message: String): SignatureOuterClass.Signature
}

fun SigningKey.createIdentity(identity: PrivateKeyOuterClass.PrivateKey): AuthorizedIdentity {
    val slimKey = PublicKeyOuterClass.PublicKey.newBuilder().apply {
        timestamp = Date().time
        secp256K1Uncompressed = identity.publicKey.secp256K1Uncompressed
    }.build()
    val signatureClass = Signature.newBuilder().build()
    val signatureText = signatureClass.createIdentityText(key = slimKey.toByteArray())
    val digest = signatureClass.ethHash(message = signatureText)
    val signature = sign(digest)

    val signatureData = KeyUtil.getSignatureData(signature.rawData.toByteString().toByteArray())
    val publicKey = Sign.recoverFromSignature(
        BigInteger(1, signatureData.v).toInt(),
        ECDSASignature(BigInteger(1, signatureData.r), BigInteger(1, signatureData.s)),
        digest,
    )

    val authorized = PublicKey.newBuilder().also {
        it.secp256K1Uncompressed = slimKey.secp256K1Uncompressed
        it.timestamp = slimKey.timestamp
        it.signature = signature
    }
    return AuthorizedIdentity(
        address = Keys.toChecksumAddress(Keys.getAddress(publicKey)),
        authorized = authorized.build(),
        identity = identity,
    )
}
