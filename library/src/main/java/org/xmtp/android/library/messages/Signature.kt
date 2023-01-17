package org.xmtp.android.library.messages

import org.bouncycastle.jcajce.provider.digest.Keccak
import org.xmtp.android.library.toHex

typealias Signature = org.xmtp.proto.message.contents.SignatureOuterClass.Signature

private const val MESSAGE_PREFIX = "\u0019Ethereum Signed Message:\n"

fun Signature.ethHash(message: String): ByteArray {
    val input = MESSAGE_PREFIX + message.length + message
    val digest256 = Keccak.Digest256()
    return digest256.digest(input.toByteArray())
}

fun Signature.createIdentityText(key: ByteArray): String =
    ("XMTP : Create Identity\n" + "${key.toHex()}\n" + "\n" + "For more info: https://xmtp.org/signatures/")

fun Signature.enableIdentityText(key: ByteArray): String =
    ("XMTP : Enable Identity\n" + "${key.toHex()}\n" + "\n" + "For more info: https://xmtp.org/signatures/")

val Signature.rawData: ByteArray
    get() = ecdsaCompact.bytes.toByteArray() + listOf(ecdsaCompact.recovery.toByte()).toByteArray()
