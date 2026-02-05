package org.xmtp.android.library

import android.util.Log
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import uniffi.xmtpv3.FfiConsent
import uniffi.xmtpv3.FfiConsentCallback
import uniffi.xmtpv3.FfiConsentEntityType
import uniffi.xmtpv3.FfiConsentState
import uniffi.xmtpv3.FfiException
import uniffi.xmtpv3.FfiPreferenceCallback
import uniffi.xmtpv3.FfiPreferenceUpdate
import uniffi.xmtpv3.FfiXmtpClient

enum class ConsentState {
    ALLOWED,
    DENIED,
    UNKNOWN,
    ;

    companion object {
        fun toFfiConsentState(option: ConsentState): FfiConsentState =
            when (option) {
                ALLOWED -> FfiConsentState.ALLOWED
                DENIED -> FfiConsentState.DENIED
                UNKNOWN -> FfiConsentState.UNKNOWN
            }

        fun fromFfiConsentState(option: FfiConsentState): ConsentState =
            when (option) {
                FfiConsentState.ALLOWED -> ALLOWED
                FfiConsentState.DENIED -> DENIED
                FfiConsentState.UNKNOWN -> UNKNOWN
            }
    }
}

enum class EntryType {
    CONVERSATION_ID,
    INBOX_ID,
    ;

    companion object {
        fun toFfiConsentEntityType(option: EntryType): FfiConsentEntityType =
            when (option) {
                CONVERSATION_ID -> FfiConsentEntityType.CONVERSATION_ID
                INBOX_ID -> FfiConsentEntityType.INBOX_ID
            }

        fun fromFfiConsentEntityType(option: FfiConsentEntityType): EntryType =
            when (option) {
                FfiConsentEntityType.CONVERSATION_ID -> CONVERSATION_ID
                FfiConsentEntityType.INBOX_ID -> INBOX_ID
            }
    }
}

enum class PreferenceType {
    HMAC_KEYS,
}

data class ConsentRecord(
    val value: String,
    val entryType: EntryType,
    val consentType: ConsentState,
) {
    companion object {
        fun conversationId(
            groupId: String,
            type: ConsentState = ConsentState.UNKNOWN,
        ): ConsentRecord = ConsentRecord(groupId, EntryType.CONVERSATION_ID, type)

        fun inboxId(
            inboxId: InboxId,
            type: ConsentState = ConsentState.UNKNOWN,
        ): ConsentRecord = ConsentRecord(inboxId, EntryType.INBOX_ID, type)
    }

    val key: String
        get() = "${entryType.name}-$value"
}

data class PrivatePreferences(
    var client: Client,
    private val ffiClient: FfiXmtpClient,
) {
    suspend fun sync() {
        ffiClient.syncPreferences()
    }

    @Deprecated(message = "Use method `sync()` instead", replaceWith = ReplaceWith("sync()"))
    suspend fun syncConsent() {
        ffiClient.sendSyncRequest()
    }

    suspend fun streamPreferenceUpdates(onClose: (() -> Unit)? = null): Flow<PreferenceType> =
        callbackFlow {
            val preferenceCallback =
                object : FfiPreferenceCallback {
                    override fun onPreferenceUpdate(preference: List<FfiPreferenceUpdate>) {
                        preference.iterator().forEach {
                            when (it) {
                                is FfiPreferenceUpdate.Hmac -> trySend(PreferenceType.HMAC_KEYS)
                            }
                        }
                    }

                    override fun onError(error: FfiException) {
                        Log.e("XMTP preference update stream", error.toString())
                    }

                    override fun onClose() {
                        onClose?.invoke()
                        close()
                    }
                }

            val stream = ffiClient.conversations().streamPreferences(preferenceCallback)

            awaitClose { stream.end() }
        }

    suspend fun streamConsent(onClose: (() -> Unit)? = null): Flow<ConsentRecord> =
        callbackFlow {
            val consentCallback =
                object : FfiConsentCallback {
                    override fun onConsentUpdate(consent: List<FfiConsent>) {
                        consent.iterator().forEach {
                            trySend(it.fromFfiConsent())
                        }
                    }

                    override fun onError(error: FfiException) {
                        Log.e("XMTP consent stream", error.toString())
                    }

                    override fun onClose() {
                        onClose?.invoke()
                        close()
                    }
                }

            val stream = ffiClient.conversations().streamConsent(consentCallback)

            awaitClose { stream.end() }
        }

    suspend fun setConsentState(entries: List<ConsentRecord>) {
        ffiClient.setConsentStates(entries.map { it.toFfiConsent() })
    }

    private fun ConsentRecord.toFfiConsent(): FfiConsent =
        FfiConsent(
            EntryType.toFfiConsentEntityType(entryType),
            ConsentState.toFfiConsentState(consentType),
            value,
        )

    private fun FfiConsent.fromFfiConsent(): ConsentRecord =
        ConsentRecord(
            entity,
            EntryType.fromFfiConsentEntityType(entityType),
            ConsentState.fromFfiConsentState(state),
        )

    suspend fun conversationState(groupId: String): ConsentState =
        ConsentState.fromFfiConsentState(
            ffiClient.getConsentState(
                FfiConsentEntityType.CONVERSATION_ID,
                groupId,
            ),
        )

    suspend fun inboxIdState(inboxId: InboxId): ConsentState =
        ConsentState.fromFfiConsentState(
            ffiClient.getConsentState(
                FfiConsentEntityType.INBOX_ID,
                inboxId,
            ),
        )
}
