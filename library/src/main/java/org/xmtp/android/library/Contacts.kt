package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.messages.ContactBundle
import org.xmtp.android.library.messages.ContactBundleBuilder
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.Pagination
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import org.xmtp.proto.message.contents.PrivatePreferences.PrivatePreferencesAction
import java.util.Date

enum class ConsentState {
    ALLOWED,
    DENIED,
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
    val entries: MutableMap<String, ConsentListEntry> = mutableMapOf()
    private val publicKey =
        client.privateKeyBundleV1.identityKey.publicKey.secp256K1Uncompressed.bytes
    private val privateKey = client.privateKeyBundleV1.identityKey.secp256K1.bytes

    private val identifier: String = uniffi.xmtpv3.generatePrivatePreferencesTopicIdentifier(
        privateKey.toByteArray()
    )

    @OptIn(ExperimentalUnsignedTypes::class)
    suspend fun load(): ConsentList {
        val envelopes = client.apiClient.envelopes(
            Topic.preferenceList(identifier).description,
            Pagination(direction = MessageApiOuterClass.SortDirection.SORT_DIRECTION_ASCENDING)
        )
        val consentList = ConsentList(client)
        val preferences: MutableList<PrivatePreferencesAction> = mutableListOf()
        for (envelope in envelopes) {
            val payload = uniffi.xmtpv3.userPreferencesDecrypt(
                publicKey.toByteArray(),
                privateKey.toByteArray(),
                envelope.message.toByteArray()
            )

            preferences.add(
                PrivatePreferencesAction.parseFrom(
                    payload.toUByteArray().toByteArray()
                )
            )
        }

        preferences.iterator().forEach { preference ->
            preference.allow?.walletAddressesList?.forEach { address ->
                consentList.allow(address)
            }
            preference.block?.walletAddressesList?.forEach { address ->
                consentList.deny(address)
            }
        }

        return consentList
    }

    fun publish(entry: ConsentListEntry) {
        val payload = PrivatePreferencesAction.newBuilder().also {
            when (entry.consentType) {
                ConsentState.ALLOWED -> it.setAllow(
                    PrivatePreferencesAction.Allow.newBuilder().addWalletAddresses(entry.value)
                )

                ConsentState.DENIED -> it.setBlock(
                    PrivatePreferencesAction.Block.newBuilder().addWalletAddresses(entry.value)
                )

                ConsentState.UNKNOWN -> it.clearMessageType()
            }
        }.build()

        val message = uniffi.xmtpv3.userPreferencesEncrypt(
            publicKey.toByteArray(),
            privateKey.toByteArray(),
            payload.toByteArray()
        )

        val envelope = EnvelopeBuilder.buildFromTopic(
            Topic.preferenceList(identifier),
            Date(),
            ByteArray(message.size) { message[it].toByte() }
        )

        client.publish(listOf(envelope))
    }

    fun allow(address: String): ConsentListEntry {
        val entry = ConsentListEntry.address(address, ConsentState.ALLOWED)
        entries[ConsentListEntry.address(address).key] = entry

        return entry
    }

    fun deny(address: String): ConsentListEntry {
        val entry = ConsentListEntry.address(address, ConsentState.DENIED)
        entries[ConsentListEntry.address(address).key] = entry

        return entry
    }

    fun state(address: String): ConsentState {
        val entry = entries[ConsentListEntry.address(address).key]

        return entry?.consentType ?: ConsentState.UNKNOWN
    }
}

data class Contacts(
    var client: Client,
    val knownBundles: MutableMap<String, ContactBundle> = mutableMapOf(),
    val hasIntroduced: MutableMap<String, Boolean> = mutableMapOf(),
) {

    var consentList: ConsentList = ConsentList(client)

    fun refreshConsentList(): ConsentList {
        runBlocking {
            consentList = ConsentList(client).load()
        }
        return consentList
    }

    fun isAllowed(address: String): Boolean {
        return consentList.state(address) == ConsentState.ALLOWED
    }

    fun isDenied(address: String): Boolean {
        return consentList.state(address) == ConsentState.DENIED
    }

    fun allow(addresses: List<String>) {
        for (address in addresses) {
            ConsentList(client).publish(consentList.allow(address))
        }
    }

    fun deny(addresses: List<String>) {
        for (address in addresses) {
            ConsentList(client).publish(consentList.deny(address))
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
