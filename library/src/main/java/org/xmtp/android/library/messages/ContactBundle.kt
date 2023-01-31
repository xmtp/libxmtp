package org.xmtp.android.library.messages

import org.bouncycastle.util.Arrays
import org.web3j.crypto.Keys
import org.xmtp.android.library.toHex
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import org.xmtp.proto.message.contents.Contact

typealias ContactBundle = org.xmtp.proto.message.contents.Contact.ContactBundle
typealias ContactBundleV1 = org.xmtp.proto.message.contents.Contact.ContactBundleV1
typealias ContactBundleV2 = org.xmtp.proto.message.contents.Contact.ContactBundleV2

class ContactBundleBuilder {
    companion object {
        fun buildFromEnvelope(envelope: MessageApiOuterClass.Envelope): ContactBundle {
            val data = envelope.message
            val contactBundle = ContactBundle.newBuilder()
            // Try to deserialize legacy v1 bundle
            val publicKeyBundle = PublicKeyBundle.parseFrom(data)
            contactBundle.v1Builder.keyBundle = publicKeyBundle
            // It's not a legacy bundle so just deserialize as a ContactBundle
            if (contactBundle.v1.keyBundle.identityKey.secp256K1Uncompressed.bytes.isEmpty) {
                contactBundle.mergeFrom(data)
            }
            return contactBundle.build()
        }
    }
}

fun ContactBundle.toPublicKeyBundle(): PublicKeyBundle {
    return when (versionCase) {
        Contact.ContactBundle.VersionCase.V1 -> v1.keyBundle
        Contact.ContactBundle.VersionCase.V2 -> PublicKeyBundleBuilder.buildFromSignedKeyBundle(v2.keyBundle)
        else -> throw IllegalArgumentException("Invalid version")
    }
}

fun ContactBundle.toSignedPublicKeyBundle(): SignedPublicKeyBundle {
    return when (versionCase) {
        Contact.ContactBundle.VersionCase.V1 -> SignedPublicKeyBundleBuilder.buildFromKeyBundle(v1.keyBundle)
        Contact.ContactBundle.VersionCase.V2 -> v2.keyBundle
        else -> throw IllegalArgumentException("Invalid version")
    }
}

val ContactBundle.walletAddress: String?
    get() {
        when (versionCase) {
            Contact.ContactBundle.VersionCase.V1 -> {
                val key = v1.keyBundle.identityKey.recoverWalletSignerPublicKey()
                val address = Keys.getAddress(
                    Arrays.copyOfRange(
                        key.secp256K1Uncompressed.bytes.toByteArray(),
                        1,
                        key.secp256K1Uncompressed.bytes.toByteArray().size
                    )
                )
                return Keys.toChecksumAddress(address.toHex())
            }
            Contact.ContactBundle.VersionCase.V2 -> {
                val key = v2.keyBundle.identityKey.recoverWalletSignerPublicKey()
                val address = Keys.getAddress(
                    Arrays.copyOfRange(
                        key.secp256K1Uncompressed.bytes.toByteArray(),
                        1,
                        key.secp256K1Uncompressed.bytes.toByteArray().size
                    )
                )
                return Keys.toChecksumAddress(address.toHex())
            }
            else -> return null
        }
    }

val ContactBundle.identityAddress: String?
    get() {
        return when (versionCase) {
            Contact.ContactBundle.VersionCase.V1 -> v1.keyBundle.identityKey.walletAddress
            Contact.ContactBundle.VersionCase.V2 -> {
                val publicKey = try {
                    PublicKeyBuilder.buildFromSignedPublicKey(v2.keyBundle.identityKey)
                } catch (e: Throwable) {
                    null
                }
                publicKey?.walletAddress
            }
            else -> null
        }
    }
