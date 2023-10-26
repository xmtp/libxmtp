package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.messages.ContactBundle
import org.xmtp.android.library.messages.ContactBundleBuilder
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.contents.PrivatePreferences.PrivatePreferencesAction
import java.util.Date

enum class ConsentState {
    ALLOWED,
    BLOCKED,
    UNKNOWN
}

data class ConsentListEntry(
    val value: String,
    val entryType: EntryType,
    val consentType: ConsentState,
) {
    enum class EntryType {
        ADDRESS
    }

    companion object {
        fun address(
            address: String,
            type: ConsentState = ConsentState.UNKNOWN,
        ): ConsentListEntry {
            return ConsentListEntry(address, EntryType.ADDRESS, type)
        }
    }

    val key: String
        get() = "${entryType.name}-$value"
}

class ConsentList(val client: Client) {
    private val entries: MutableMap<String, ConsentState> = mutableMapOf()
    private val publicKey =
        client.privateKeyBundleV1.identityKey.publicKey.secp256K1Uncompressed.bytes
    private val privateKey = client.privateKeyBundleV1.identityKey.secp256K1.bytes

    @OptIn(ExperimentalUnsignedTypes::class)
    private val identifier: String = uniffi.xmtp_dh.generatePrivatePreferencesTopicIdentifier(
        privateKey.toByteArray().toUByteArray().toList()
    )

    @OptIn(ExperimentalUnsignedTypes::class)
    suspend fun load(): ConsentList {
        val envelopes = client.query(Topic.preferenceList(identifier))
        val consentList = ConsentList(client)
        val preferences: MutableList<PrivatePreferencesAction> = mutableListOf()

        for (envelope in envelopes.envelopesList) {
            val payload = uniffi.xmtp_dh.eciesDecryptK256Sha3256(
                publicKey.toByteArray().toUByteArray().toList(),
                privateKey.toByteArray().toUByteArray().toList(),
                envelope.message.toByteArray().toUByteArray().toList()
            )

            preferences.add(
                PrivatePreferencesAction.parseFrom(
                    payload.toUByteArray().toByteArray()
                )
            )
        }

        preferences.reversed().iterator().forEach { preference ->
            preference.allow?.walletAddressesList?.forEach { address ->
                consentList.allow(address)
            }
            preference.block?.walletAddressesList?.forEach { address ->
                consentList.block(address)
            }
        }

        return consentList
    }

    @OptIn(ExperimentalUnsignedTypes::class)
    fun publish(entry: ConsentListEntry) {
        val payload = PrivatePreferencesAction.newBuilder().also {
            when (entry.consentType) {
                ConsentState.ALLOWED -> it.setAllow(
                    PrivatePreferencesAction.Allow.newBuilder().addWalletAddresses(entry.value)
                )

                ConsentState.BLOCKED -> it.setBlock(
                    PrivatePreferencesAction.Block.newBuilder().addWalletAddresses(entry.value)
                )

                ConsentState.UNKNOWN -> it.clearMessageType()
            }
        }.build()

        val message = uniffi.xmtp_dh.eciesEncryptK256Sha3256(
            publicKey.toByteArray().toUByteArray().toList(),
            privateKey.toByteArray().toUByteArray().toList(),
            payload.toByteArray().toUByteArray().toList()
        )

        val envelope = EnvelopeBuilder.buildFromTopic(
            Topic.preferenceList(identifier),
            Date(),
            ByteArray(message.size) { message[it].toByte() }
        )

        client.publish(listOf(envelope))
    }

    fun allow(address: String): ConsentListEntry {
        entries[ConsentListEntry.address(address).key] = ConsentState.ALLOWED

        return ConsentListEntry.address(address, ConsentState.ALLOWED)
    }

    fun block(address: String): ConsentListEntry {
        entries[ConsentListEntry.address(address).key] = ConsentState.BLOCKED

        return ConsentListEntry.address(address, ConsentState.BLOCKED)
    }

    fun state(address: String): ConsentState {
        val state = entries[ConsentListEntry.address(address).key]

        return state ?: ConsentState.UNKNOWN
    }
}

data class Contacts(
    var client: Client,
    val knownBundles: MutableMap<String, ContactBundle> = mutableMapOf(),
    val hasIntroduced: MutableMap<String, Boolean> = mutableMapOf(),
) {

    var consentList: ConsentList = ConsentList(client)

    fun refreshConsentList() {
        runBlocking {
            consentList = ConsentList(client).load()
        }
    }

    fun isAllowed(address: String): Boolean {
        return consentList.state(address) == ConsentState.ALLOWED
    }

    fun isBlocked(address: String): Boolean {
        return consentList.state(address) == ConsentState.BLOCKED
    }

    fun allow(addresses: List<String>) {
        for (address in addresses) {
            ConsentList(client).publish(consentList.allow(address))
        }
    }

    fun block(addresses: List<String>) {
        for (address in addresses) {
            ConsentList(client).publish(consentList.block(address))
        }
    }

    fun has(peerAddress: String): Boolean =
        knownBundles[peerAddress] != null

    fun needsIntroduction(peerAddress: String): Boolean =
        hasIntroduced[peerAddress] != true

    fun find(peerAddress: String): ContactBundle? {
        val knownBundle = knownBundles[peerAddress]
        if (knownBundle != null) {
            return knownBundle
        }
        val response = runBlocking { client.query(topic = Topic.contact(peerAddress)) }

        if (response.envelopesList.isNullOrEmpty()) return null

        for (envelope in response.envelopesList) {
            val contactBundle = ContactBundleBuilder.buildFromEnvelope(envelope)
            knownBundles[peerAddress] = contactBundle
            val address = contactBundle.walletAddress
            if (address == peerAddress) {
                return contactBundle
            }
        }

        return null
    }
}
