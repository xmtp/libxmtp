package org.xmtp.android.library

import com.google.protobuf.kotlin.toByteStringUtf8
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
    UNKNOWN,
}

data class ConsentListEntry(
    val value: String,
    val entryType: EntryType,
    val consentType: ConsentState,
) {
    enum class EntryType {
        ADDRESS,
        GROUP_ID,
    }

    companion object {
        fun address(
            address: String,
            type: ConsentState = ConsentState.UNKNOWN,
        ): ConsentListEntry {
            return ConsentListEntry(address, EntryType.ADDRESS, type)
        }

        fun groupId(
            groupId: ByteArray,
            type: ConsentState = ConsentState.UNKNOWN,
        ): ConsentListEntry {
            return ConsentListEntry(String(groupId), EntryType.GROUP_ID, type)
        }
    }

    val key: String
        get() = "${entryType.name}-$value"
}

class ConsentList(
    val client: Client,
    val entries: MutableMap<String, ConsentListEntry> = mutableMapOf(),
) {
    private var lastFetched: Date? = null
    private val publicKey =
        client.privateKeyBundleV1.identityKey.publicKey.secp256K1Uncompressed.bytes
    private val privateKey = client.privateKeyBundleV1.identityKey.secp256K1.bytes

    private val identifier: String =
        uniffi.xmtpv3.generatePrivatePreferencesTopicIdentifier(
            privateKey.toByteArray(),
        )

    @OptIn(ExperimentalUnsignedTypes::class)
    suspend fun load(): List<ConsentListEntry> {
        val newDate = Date()
        val envelopes =
            client.apiClient.envelopes(
                Topic.preferenceList(identifier).description,
                Pagination(
                    after = lastFetched,
                    direction = MessageApiOuterClass.SortDirection.SORT_DIRECTION_ASCENDING
                ),
            )
        lastFetched = newDate
        val preferences: MutableList<PrivatePreferencesAction> = mutableListOf()
        for (envelope in envelopes) {
            val payload =
                uniffi.xmtpv3.userPreferencesDecrypt(
                    publicKey.toByteArray(),
                    privateKey.toByteArray(),
                    envelope.message.toByteArray(),
                )

            preferences.add(
                PrivatePreferencesAction.parseFrom(
                    payload.toUByteArray().toByteArray(),
                )
            )
        }

        preferences.iterator().forEach { preference ->
            preference.allowAddress?.walletAddressesList?.forEach { address ->
                allow(address)
            }
            preference.denyAddress?.walletAddressesList?.forEach { address ->
                deny(address)
            }
            preference.allowGroup?.groupIdsList?.forEach { groupId ->
                allowGroup(groupId.toByteArray())
            }
            preference.denyGroup?.groupIdsList?.forEach { groupId ->
                denyGroup(groupId.toByteArray())
            }
        }

        return entries.values.toList()
    }

    suspend fun publish(entries: List<ConsentListEntry>) {
        val payload = PrivatePreferencesAction.newBuilder().also {
            entries.forEach { entry ->
                when (entry.entryType) {
                    ConsentListEntry.EntryType.ADDRESS -> {
                        when (entry.consentType) {
                            ConsentState.ALLOWED ->
                                it.setAllowAddress(
                                    PrivatePreferencesAction.AllowAddress.newBuilder()
                                        .addWalletAddresses(entry.value),
                                )

                            ConsentState.DENIED ->
                                it.setDenyAddress(
                                    PrivatePreferencesAction.DenyAddress.newBuilder()
                                        .addWalletAddresses(entry.value),
                                )

                            ConsentState.UNKNOWN -> it.clearMessageType()
                        }
                    }

                    ConsentListEntry.EntryType.GROUP_ID -> {
                        when (entry.consentType) {
                            ConsentState.ALLOWED ->
                                it.setAllowGroup(
                                    PrivatePreferencesAction.AllowGroup.newBuilder()
                                        .addGroupIds(entry.value.toByteStringUtf8()),
                                )

                            ConsentState.DENIED ->
                                it.setDenyGroup(
                                    PrivatePreferencesAction.DenyGroup.newBuilder()
                                        .addGroupIds(entry.value.toByteStringUtf8()),
                                )

                            ConsentState.UNKNOWN -> it.clearMessageType()
                        }
                    }
                }
            }
        }.build()

        val message =
            uniffi.xmtpv3.userPreferencesEncrypt(
                publicKey.toByteArray(),
                privateKey.toByteArray(),
                payload.toByteArray(),
            )

        val envelope = EnvelopeBuilder.buildFromTopic(
            Topic.preferenceList(identifier),
            Date(),
            ByteArray(message.size) { message[it] },
        )

        client.publish(listOf(envelope))
    }

    fun allow(address: String): ConsentListEntry {
        val entry = ConsentListEntry.address(address, ConsentState.ALLOWED)
        entries[entry.key] = entry

        return entry
    }

    fun deny(address: String): ConsentListEntry {
        val entry = ConsentListEntry.address(address, ConsentState.DENIED)
        entries[entry.key] = entry

        return entry
    }

    fun allowGroup(groupId: ByteArray): ConsentListEntry {
        val entry = ConsentListEntry.groupId(groupId, ConsentState.ALLOWED)
        entries[entry.key] = entry

        return entry
    }

    fun denyGroup(groupId: ByteArray): ConsentListEntry {
        val entry = ConsentListEntry.groupId(groupId, ConsentState.DENIED)
        entries[entry.key] = entry

        return entry
    }

    fun state(address: String): ConsentState {
        val entry = entries[ConsentListEntry.address(address).key]

        return entry?.consentType ?: ConsentState.UNKNOWN
    }

    fun groupState(groupId: ByteArray): ConsentState {
        val entry = entries[ConsentListEntry.groupId(groupId).key]

        return entry?.consentType ?: ConsentState.UNKNOWN
    }
}

data class Contacts(
    var client: Client,
    val knownBundles: MutableMap<String, ContactBundle> = mutableMapOf(),
    val hasIntroduced: MutableMap<String, Boolean> = mutableMapOf(),
    var consentList: ConsentList = ConsentList(client),
) {

    suspend fun refreshConsentList(): ConsentList {
        consentList.load()
        return consentList
    }

    suspend fun allow(addresses: List<String>) {
        val entries = addresses.map {
            consentList.allow(it)
        }
        consentList.publish(entries)
    }

    suspend fun deny(addresses: List<String>) {
        val entries = addresses.map {
            consentList.deny(it)
        }
        consentList.publish(entries)
    }

    suspend fun allowGroup(groupIds: List<ByteArray>) {
        val entries = groupIds.map {
            consentList.allowGroup(it)
        }
        consentList.publish(entries)
    }

    suspend fun denyGroup(groupIds: List<ByteArray>) {
        val entries = groupIds.map {
            consentList.denyGroup(it)
        }
        consentList.publish(entries)
    }

    fun isAllowed(address: String): Boolean {
        return consentList.state(address) == ConsentState.ALLOWED
    }

    fun isDenied(address: String): Boolean {
        return consentList.state(address) == ConsentState.DENIED
    }

    fun isGroupAllowed(groupId: ByteArray): Boolean {
        return consentList.groupState(groupId) == ConsentState.ALLOWED
    }

    fun isGroupDenied(groupId: ByteArray): Boolean {
        return consentList.groupState(groupId) == ConsentState.DENIED
    }

    fun has(peerAddress: String): Boolean = knownBundles[peerAddress] != null

    fun needsIntroduction(peerAddress: String): Boolean = hasIntroduced[peerAddress] != true

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
            if (address?.lowercase() == peerAddress.lowercase()) {
                return contactBundle
            }
        }

        return null
    }
}
