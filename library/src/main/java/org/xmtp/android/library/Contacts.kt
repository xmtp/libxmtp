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
import uniffi.xmtpv3.FfiConsent
import uniffi.xmtpv3.FfiConsentEntityType
import uniffi.xmtpv3.FfiConsentState
import java.util.Date

enum class ConsentState {
    ALLOWED,
    DENIED,
    UNKNOWN;

    companion object {
        fun toFfiConsentState(option: ConsentState): FfiConsentState {
            return when (option) {
                ConsentState.ALLOWED -> FfiConsentState.ALLOWED
                ConsentState.DENIED -> FfiConsentState.DENIED
                else -> FfiConsentState.UNKNOWN
            }
        }

        fun fromFfiConsentState(option: FfiConsentState): ConsentState {
            return when (option) {
                FfiConsentState.ALLOWED -> ConsentState.ALLOWED
                FfiConsentState.DENIED -> ConsentState.DENIED
                else -> ConsentState.UNKNOWN
            }
        }
    }
}

enum class EntryType {
    ADDRESS,
    GROUP_ID,
    INBOX_ID;

    companion object {
        fun toFfiConsentEntityType(option: EntryType): FfiConsentEntityType {
            return when (option) {
                EntryType.ADDRESS -> FfiConsentEntityType.ADDRESS
                EntryType.GROUP_ID -> FfiConsentEntityType.CONVERSATION_ID
                EntryType.INBOX_ID -> FfiConsentEntityType.INBOX_ID
            }
        }

        fun fromFfiConsentEntityType(option: FfiConsentEntityType): EntryType {
            return when (option) {
                FfiConsentEntityType.ADDRESS -> EntryType.ADDRESS
                FfiConsentEntityType.CONVERSATION_ID -> EntryType.GROUP_ID
                FfiConsentEntityType.INBOX_ID -> EntryType.INBOX_ID
            }
        }
    }
}

data class ConsentListEntry(
    val value: String,
    val entryType: EntryType,
    val consentType: ConsentState,
) {
    companion object {
        fun address(
            address: String,
            type: ConsentState = ConsentState.UNKNOWN,
        ): ConsentListEntry {
            return ConsentListEntry(address, EntryType.ADDRESS, type)
        }

        fun groupId(
            groupId: String,
            type: ConsentState = ConsentState.UNKNOWN,
        ): ConsentListEntry {
            return ConsentListEntry(groupId, EntryType.GROUP_ID, type)
        }

        fun inboxId(
            inboxId: String,
            type: ConsentState = ConsentState.UNKNOWN,
        ): ConsentListEntry {
            return ConsentListEntry(inboxId, EntryType.INBOX_ID, type)
        }
    }

    fun toFfiConsent(): FfiConsent {
        return FfiConsent(
            EntryType.toFfiConsentEntityType(entryType),
            ConsentState.toFfiConsentState(consentType),
            value
        )
    }

    val key: String
        get() = "${entryType.name}-$value"
}

class ConsentList(
    val client: Client,
    val entries: MutableMap<String, ConsentListEntry> = mutableMapOf(),
) {
    private var lastFetched: Date? = null

    @OptIn(ExperimentalUnsignedTypes::class)
    suspend fun load(): List<ConsentListEntry> {
        if (client.hasV2Client) {
            val newDate = Date()
            val publicKey =
                client.v1keys.identityKey.publicKey.secp256K1Uncompressed.bytes
            val privateKey = client.v1keys.identityKey.secp256K1.bytes
            val identifier: String =
                uniffi.xmtpv3.generatePrivatePreferencesTopicIdentifier(
                    privateKey.toByteArray(),
                )
            val envelopes =
                client.apiClient!!.envelopes(
                    Topic.preferenceList(identifier).description,
                    Pagination(
                        after = lastFetched,
                        direction = MessageApiOuterClass.SortDirection.SORT_DIRECTION_ASCENDING,
                        limit = 500
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
                    allowGroup(groupId)
                }
                preference.denyGroup?.groupIdsList?.forEach { groupId ->
                    denyGroup(groupId)
                }

                preference.allowInboxId?.inboxIdsList?.forEach { inboxId ->
                    allowInboxId(inboxId)
                }
                preference.denyInboxId?.inboxIdsList?.forEach { inboxId ->
                    denyInboxId(inboxId)
                }
            }
        }
        return entries.values.toList()
    }

    suspend fun publish(entries: List<ConsentListEntry>) {
        if (client.v3Client != null) {
            setV3ConsentState(entries)
        }
        if (client.hasV2Client) {
            val payload = PrivatePreferencesAction.newBuilder().also {
                entries.iterator().forEach { entry ->
                    when (entry.entryType to entry.consentType) {
                        EntryType.ADDRESS to ConsentState.ALLOWED -> it.setAllowAddress(
                            PrivatePreferencesAction.AllowAddress.newBuilder()
                                .addWalletAddresses(entry.value)
                        )

                        EntryType.ADDRESS to ConsentState.DENIED -> it.setDenyAddress(
                            PrivatePreferencesAction.DenyAddress.newBuilder()
                                .addWalletAddresses(entry.value)
                        )

                        EntryType.GROUP_ID to ConsentState.ALLOWED -> it.setAllowGroup(
                            PrivatePreferencesAction.AllowGroup.newBuilder()
                                .addGroupIds(entry.value)
                        )

                        EntryType.GROUP_ID to ConsentState.DENIED -> it.setDenyGroup(
                            PrivatePreferencesAction.DenyGroup.newBuilder().addGroupIds(entry.value)
                        )

                        EntryType.INBOX_ID to ConsentState.ALLOWED -> it.setAllowInboxId(
                            PrivatePreferencesAction.AllowInboxId.newBuilder()
                                .addInboxIds(entry.value)
                        )

                        EntryType.INBOX_ID to ConsentState.DENIED -> it.setDenyInboxId(
                            PrivatePreferencesAction.DenyInboxId.newBuilder()
                                .addInboxIds(entry.value)
                        )

                        else -> it.clearMessageType()
                    }
                }
            }.build()

            val publicKey =
                client.v1keys.identityKey.publicKey.secp256K1Uncompressed.bytes
            val privateKey = client.v1keys.identityKey.secp256K1.bytes
            val identifier: String =
                uniffi.xmtpv3.generatePrivatePreferencesTopicIdentifier(
                    privateKey.toByteArray(),
                )

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
    }

    suspend fun setV3ConsentState(entries: List<ConsentListEntry>) {
        client.v3Client?.setConsentStates(entries.map { it.toFfiConsent() })
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

    fun allowGroup(groupId: String): ConsentListEntry {
        val entry = ConsentListEntry.groupId(groupId, ConsentState.ALLOWED)
        entries[entry.key] = entry

        return entry
    }

    fun denyGroup(groupId: String): ConsentListEntry {
        val entry = ConsentListEntry.groupId(groupId, ConsentState.DENIED)
        entries[entry.key] = entry

        return entry
    }

    fun allowInboxId(inboxId: String): ConsentListEntry {
        val entry = ConsentListEntry.inboxId(inboxId, ConsentState.ALLOWED)
        entries[entry.key] = entry

        return entry
    }

    fun denyInboxId(inboxId: String): ConsentListEntry {
        val entry = ConsentListEntry.inboxId(inboxId, ConsentState.DENIED)
        entries[entry.key] = entry

        return entry
    }

    suspend fun state(address: String): ConsentState {
        client.v3Client?.let {
            return ConsentState.fromFfiConsentState(
                it.getConsentState(
                    FfiConsentEntityType.ADDRESS,
                    address
                )
            )
        }
        val entry = entries[ConsentListEntry.address(address).key]
        return entry?.consentType ?: ConsentState.UNKNOWN
    }

    suspend fun groupState(groupId: String): ConsentState {
        client.v3Client?.let {
            return ConsentState.fromFfiConsentState(
                it.getConsentState(
                    FfiConsentEntityType.CONVERSATION_ID,
                    groupId
                )
            )
        }
        val entry = entries[ConsentListEntry.groupId(groupId).key]
        return entry?.consentType ?: ConsentState.UNKNOWN
    }

    suspend fun inboxIdState(inboxId: String): ConsentState {
        client.v3Client?.let {
            return ConsentState.fromFfiConsentState(
                it.getConsentState(
                    FfiConsentEntityType.INBOX_ID,
                    inboxId
                )
            )
        }
        val entry = entries[ConsentListEntry.inboxId(inboxId).key]
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
        val entries = consentList.load()
        consentList.setV3ConsentState(entries)
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

    suspend fun allowGroups(groupIds: List<String>) {
        val entries = groupIds.map {
            consentList.allowGroup(it)
        }
        consentList.publish(entries)
    }

    suspend fun denyGroups(groupIds: List<String>) {
        val entries = groupIds.map {
            consentList.denyGroup(it)
        }
        consentList.publish(entries)
    }

    suspend fun allowInboxes(inboxIds: List<String>) {
        val entries = inboxIds.map {
            consentList.allowInboxId(it)
        }
        consentList.publish(entries)
    }

    suspend fun denyInboxes(inboxIds: List<String>) {
        val entries = inboxIds.map {
            consentList.denyInboxId(it)
        }
        consentList.publish(entries)
    }

    suspend fun isAllowed(address: String): Boolean {
        return consentList.state(address) == ConsentState.ALLOWED
    }

    suspend fun isDenied(address: String): Boolean {
        return consentList.state(address) == ConsentState.DENIED
    }

    suspend fun isGroupAllowed(groupId: String): Boolean {
        return consentList.groupState(groupId) == ConsentState.ALLOWED
    }

    suspend fun isGroupDenied(groupId: String): Boolean {
        return consentList.groupState(groupId) == ConsentState.DENIED
    }

    suspend fun isInboxAllowed(inboxId: String): Boolean {
        return consentList.inboxIdState(inboxId) == ConsentState.ALLOWED
    }

    suspend fun isInboxDenied(inboxId: String): Boolean {
        return consentList.inboxIdState(inboxId) == ConsentState.DENIED
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
