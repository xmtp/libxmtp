package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.messages.ContactBundle
import org.xmtp.android.library.messages.ContactBundleBuilder
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.contents.PrivatePreferences.PrivatePreferencesAction
import java.util.Date

typealias MessageType = PrivatePreferencesAction.MessageTypeCase

enum class AllowState {
    ALLOW,
    BLOCK,
    UNKNOWN
}
data class AllowListEntry(
    val value: String,
    val entryType: EntryType,
    val permissionType: AllowState,
) {
    enum class EntryType {
        ADDRESS
    }

    companion object {
        fun address(
            address: String,
            type: AllowState = AllowState.UNKNOWN,
        ): AllowListEntry {
            return AllowListEntry(address, EntryType.ADDRESS, type)
        }
    }

    val key: String
        get() = "${entryType.name}-$value"
}

class AllowList(val client: Client) {
    private val entries: MutableMap<String, AllowState> = mutableMapOf()
    private val publicKey =
        client.privateKeyBundleV1.identityKey.publicKey.secp256K1Uncompressed.bytes
    private val privateKey = client.privateKeyBundleV1.identityKey.secp256K1.bytes

    @OptIn(ExperimentalUnsignedTypes::class)
    private val identifier: String = uniffi.xmtp_dh.generatePrivatePreferencesTopicIdentifier(
        privateKey.toByteArray().toUByteArray().toList()
    )

    @OptIn(ExperimentalUnsignedTypes::class)
    suspend fun load(): AllowList {
        val envelopes = client.query(Topic.preferenceList(identifier))
        val allowList = AllowList(client)
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

        preferences.iterator().forEach { preference ->
            preference.allow?.walletAddressesList?.forEach { address ->
                allowList.allow(address)
            }
            preference.block?.walletAddressesList?.forEach { address ->
                allowList.block(address)
            }
        }
        return allowList
    }

    @OptIn(ExperimentalUnsignedTypes::class)
    fun publish(entry: AllowListEntry) {
        val payload = PrivatePreferencesAction.newBuilder().also {
            when (entry.permissionType) {
                AllowState.ALLOW -> it.setAllow(PrivatePreferencesAction.Allow.newBuilder().addWalletAddresses(entry.value))
                AllowState.BLOCK -> it.setBlock(PrivatePreferencesAction.Block.newBuilder().addWalletAddresses(entry.value))
                AllowState.UNKNOWN -> it.clearMessageType()
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

    fun allow(address: String): AllowListEntry {
        entries[AllowListEntry.address(address).key] = AllowState.ALLOW

        return AllowListEntry.address(address, AllowState.ALLOW)
    }

    fun block(address: String): AllowListEntry {
        entries[AllowListEntry.address(address).key] = AllowState.BLOCK

        return AllowListEntry.address(address, AllowState.BLOCK)
    }

    fun state(address: String): AllowState {
        val state = entries[AllowListEntry.address(address).key]

        return state ?: AllowState.UNKNOWN
    }
}

data class Contacts(
    var client: Client,
    val knownBundles: MutableMap<String, ContactBundle> = mutableMapOf(),
    val hasIntroduced: MutableMap<String, Boolean> = mutableMapOf(),
) {

    var allowList: AllowList = AllowList(client)

    fun refreshAllowList() {
        runBlocking {
            allowList = AllowList(client).load()
        }
    }

    fun isAllowed(address: String): Boolean {
        return allowList.state(address) == AllowState.ALLOW
    }

    fun isBlocked(address: String): Boolean {
        return allowList.state(address) == AllowState.BLOCK
    }

    fun allow(addresses: List<String>) {
        for (address in addresses) {
            AllowList(client).publish(allowList.allow(address))
        }
    }

    fun block(addresses: List<String>) {
        for (address in addresses) {
            AllowList(client).publish(allowList.block(address))
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
