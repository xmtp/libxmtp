package org.xmtp.android.example.messenger
import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.core.stringSetPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.json.JSONArray
import org.json.JSONObject
import org.xmtp.android.example.shared.ScrollAnchor

private val Context
.messengerDataStore by preferencesDataStore("messenger")

/** Small references only
. Message bodies stay in SDK storage. */
class MessengerPreferences internal constructor(
    private val store: DataStore<Preferences>,
) {
    constructor(context: Context) : this(context.applicationContext.messengerDataStore)

    internal var beforeDraftCommit: suspend () -> Unit = {}
    internal var beforeSessionCommit: suspend () -> Unit = {}
    internal var sessionCommitAccepted: (BackendProfile) -> Unit = {}
    internal var beforePositionCommit: suspend (String) -> Unit = {}
    internal var beforeDraftAdmission: suspend () -> Unit = {}
    internal var positionCommitFinished: (String, Boolean) -> Unit = { _, _ -> }

    suspend fun commitSession(
        profile: BackendProfile,
        admit: (() -> Unit) -> Boolean,
    ): Boolean {
        var accepted = false
        store.edit { values ->
            beforeSessionCommit()
            accepted =
                admit {
                    val existing = JSONArray(values[stringPreferencesKey("profiles")] ?: "[]")
                    val updated = JSONArray()
                    for (index in 0 until existing.length()) {
                        val item = existing.getJSONObject(index)
                        if (item.getString("id") != profile.id) updated.put(item)
                    }
                    updated.put(
                        JSONObject()
                            .put("id", profile.id)
                            .put("backend", profile.backend)
                            .put("inbox", profile.inboxId ?: "")
                            .put("identity", profile.identity ?: "")
                            .put("private", profile.allowPrivateNetwork),
                    )
                    values[stringPreferencesKey("profiles")] = updated.toString()
                    values[stringPreferencesKey("active")] = profile.id
                    values[stringPreferencesKey("signed-in")] = "true"
                    sessionCommitAccepted(profile)
                }
        }
        return accepted
    }

    private suspend fun get(key: String) =
        store.data
            .first()[stringPreferencesKey(key)]

    private suspend fun put(
        key: String,
        value: String?,
        admit: (() -> Unit) -> Boolean = { change ->
            change()
            true
        },
        position: Boolean = false,
    ): Boolean {
        var accepted = false
        store.edit {
            if (position) beforePositionCommit(key)
            accepted =
                admit {
                    if (value ==
                        null
                    ) {
                        it
                            .remove(stringPreferencesKey(key))
                    } else {
                        it[stringPreferencesKey(key)] = value
                    }
                }
        }
        if (position) positionCommitFinished(key, accepted)
        return accepted
    }

    suspend fun profiles(): List<BackendProfile> {
        val array = JSONArray(get("profiles") ?: "[]")
        return (
            0 until
                array
                    .length()
        ).map { i ->
            array
                .getJSONObject(i)
                .let {
                    BackendProfile(
                        it
                            .getString("id"),
                        it
                            .getString("backend"),
                        it
                            .optString("inbox")
                            .takeIf(String::isNotEmpty),
                        it
                            .optString("identity")
                            .takeIf(String::isNotEmpty),
                        it
                            .optBoolean("private"),
                    )
                }
        }
    }

    suspend fun saveProfile(profile: BackendProfile) {
        val values =
            profiles().filterNot {
                it.id ==
                    profile.id
            } + profile
        put(
            "profiles",
            JSONArray()
                .apply {
                    values.forEach {
                        put(
                            JSONObject()
                                .put(
                                    "id",
                                    it.id,
                                ).put(
                                    "backend",
                                    it.backend,
                                ).put(
                                    "inbox",
                                    it.inboxId ?: "",
                                ).put(
                                    "identity",
                                    it.identity ?: "",
                                ).put(
                                    "private",
                                    it.allowPrivateNetwork,
                                ),
                        )
                    }
                }.toString(),
        )
    }

    suspend fun active(): BackendProfile? =
        profiles().firstOrNull {
            it.id == get("active")
        }

    suspend fun setActive(profile: BackendProfile) {
        saveProfile(profile)
        put(
            "active",
            profile.id,
        )
    }

    suspend fun signedIn() = get("signed-in") == "true"

    suspend fun setSignedIn(value: Boolean) =
        put(
            "signed-in",
            value
                .toString(),
        )

    suspend fun beginExportCleanup(profileId: String?) {
        store.edit { values ->
            values[stringPreferencesKey("signed-in")] = "false"
            if (profileId != null) {
                val key = stringSetPreferencesKey("pending-export-cleanup")
                values[key] = values[key].orEmpty() + profileId
            }
        }
    }

    suspend fun pendingExportCleanup(): Set<String> =
        store.data
            .first()[stringSetPreferencesKey("pending-export-cleanup")]
            .orEmpty()
            .toSet()

    suspend fun completeExportCleanup(profileId: String) {
        store.edit { values ->
            val key = stringSetPreferencesKey("pending-export-cleanup")
            val remaining = values[key].orEmpty() - profileId
            if (remaining.isEmpty()) values.remove(key) else values[key] = remaining
        }
    }

    suspend fun reset(): ResetRecord? =
        get("reset")?.let {
            JSONObject(it).let { o ->
                ResetRecord(
                    o
                        .getString("profile"),
                    o
                        .getString("db"),
                    o
                        .getJSONArray("files")
                        .let { a ->
                            (
                                0 until
                                    a
                                        .length()
                            ).map(a::getString)
                        },
                    ResetPhase
                        .valueOf(
                            o
                                .getString("phase"),
                        ),
                )
            }
        }

    suspend fun saveReset(value: ResetRecord?) =
        put(
            "reset",
            value?.let {
                JSONObject()
                    .put(
                        "profile",
                        it.profileId,
                    ).put(
                        "db",
                        it.ownedDatabasePath,
                    ).put(
                        "files",
                        JSONArray(
                            it.ownedFiles,
                        ),
                    ).put(
                        "phase",
                        it.phase.name,
                    ).toString()
            },
        )

    suspend fun marker(
        profile: String,
        conversation: String,
    ) = ReadMarker(get("$profile/read/$conversation")?.toLong())

    suspend fun saveMarker(
        profile: String,
        conversation: String,
        value: Long,
        admit: (() -> Unit) -> Boolean = { change ->
            change()
            true
        },
    ) = put(
        "$profile/read/$conversation",
        value
            .toString(),
        admit,
        position = true,
    )

    suspend fun anchor(
        profile: String,
        conversation: String,
    ): ScrollAnchor? =
        get("$profile/scroll/$conversation")?.let {
            JSONObject(
                it,
            ).let { o ->
                ScrollAnchor(
                    o
                        .getString("id"),
                    o
                        .getLong("ns"),
                    o
                        .getInt("px"),
                    o
                        .getBoolean("newest"),
                    o.optString("cursor").takeIf { cursor -> cursor.isNotEmpty() },
                )
            }
        }

    suspend fun clearAnchor(
        profile: String,
        conversation: String,
        admit: (() -> Unit) -> Boolean,
    ) = put("$profile/scroll/$conversation", null, admit, position = true)

    suspend fun saveAnchor(
        profile: String,
        conversation: String,
        value: ScrollAnchor,
        admit: (() -> Unit) -> Boolean = { change ->
            change()
            true
        },
    ) = put(
        "$profile/scroll/$conversation",
        JSONObject()
            .put(
                "id",
                value.messageId,
            ).put(
                "ns",
                value.sentAtNs,
            ).put(
                "px",
                value.offsetPx,
            ).put(
                "newest",
                value.wasAtNewest,
            ).put(
                "cursor",
                value.deliveryCursor,
            ).toString(),
        admit,
        position = true,
    )

    private fun draftEntries(encoded: String?): List<SendDraftRef> {
        val array = JSONArray(encoded ?: "[]")
        return (
            0 until
                array
                    .length()
        ).map { i ->
            array
                .getJSONObject(i)
                .let {
                    SendDraftRef(
                        it
                            .getString("id"),
                        it
                            .getString("conversation"),
                        it
                            .optString("secret")
                            .takeIf(String::isNotEmpty),
                        it
                            .optString("accepted")
                            .takeIf(String::isNotEmpty),
                        SendPhase
                            .valueOf(
                                it
                                    .getString("phase"),
                            ),
                    )
                }
        }
    }

    suspend fun drafts(profile: String): List<SendDraftRef> = draftEntries(get("$profile/drafts"))

    // The callback is synchronous. It must not read storage or call the SDK.
    internal suspend fun admitDraftSnapshot(
        profile: String,
        snapshot: SendDraftRef,
        change: () -> Unit,
    ) {
        store.edit { values ->
            if (snapshot in draftEntries(values[stringPreferencesKey("$profile/drafts")])) change()
        }
    }

    suspend fun saveDraft(
        profile: String,
        draft: SendDraftRef,
        admit: (() -> Unit) -> Boolean = {
            it()
            true
        },
    ) = mutateDrafts(profile, admit) { entries -> entries.filterNot { it.draftId == draft.draftId } + draft }

    suspend fun removeDraft(
        profile: String,
        id: String,
        admit: (() -> Unit) -> Boolean = {
            it()
            true
        },
    ) = mutateDrafts(profile, admit) { entries -> entries.filterNot { it.draftId == id } }

    suspend fun saveAcceptedDraft(
        profile: String,
        draft: SendDraftRef,
    ): Boolean {
        var matched = false
        mutateDrafts(profile, { change ->
            change()
            true
        }) { entries ->
            entries.map { current ->
                if (current.draftId == draft.draftId && current.conversationKey == draft.conversationKey &&
                    (current.phase == SendPhase.QUEUEING || current.acceptedMessageId == draft.acceptedMessageId)
                ) {
                    matched = true
                    current.copy(phase = SendPhase.ACCEPTED, acceptedMessageId = draft.acceptedMessageId)
                } else {
                    current
                }
            }
        }
        return matched
    }

    internal suspend fun prepareQueueDraft(
        profile: String,
        draft: SendDraftRef,
        admit: (() -> Unit) -> Boolean,
    ): Pair<Boolean, SendDraftRef?> {
        var prepared = false
        var previous: SendDraftRef? = null
        mutateDrafts(profile, admit) { entries ->
            previous = entries.firstOrNull { it.draftId == draft.draftId }
            if (draft.descriptorSecretRef != null && previous != draft) {
                entries
            } else {
                prepared = true
                entries.filterNot { it.draftId == draft.draftId } + draft.copy(phase = SendPhase.QUEUEING)
            }
        }
        return prepared to previous
    }

    internal suspend fun rejectQueueDraft(
        profile: String,
        queued: SendDraftRef,
        previous: SendDraftRef?,
        admit: (() -> Unit) -> Boolean,
    ) = mutateDrafts(profile, admit) { entries ->
        entries.mapNotNull { entry ->
            if (entry != queued) entry else previous?.takeIf { it.descriptorSecretRef != null }
        }
    }

    private suspend fun mutateDrafts(
        profile: String,
        admit: (() -> Unit) -> Boolean,
        transform: (List<SendDraftRef>) -> List<SendDraftRef>,
    ): Boolean {
        var accepted = false
        store.edit { values ->
            beforeDraftCommit()
            beforeDraftAdmission()
            accepted =
                admit {
                    val entries = draftEntries(values[stringPreferencesKey("$profile/drafts")])
                    val updated = JSONArray()
                    transform(entries).forEach {
                        updated.put(
                            JSONObject()
                                .put("id", it.draftId)
                                .put("conversation", it.conversationKey)
                                .put("secret", it.descriptorSecretRef ?: "")
                                .put("accepted", it.acceptedMessageId ?: "")
                                .put("phase", it.phase.name),
                        )
                    }
                    values[stringPreferencesKey("$profile/drafts")] = updated.toString()
                }
        }
        return accepted
    }

    suspend fun removeProfile(id: String) {
        put(
            "profiles",
            JSONArray()
                .apply {
                    profiles()
                        .filterNot {
                            it.id == id
                        }.forEach {
                            put(
                                JSONObject()
                                    .put(
                                        "id",
                                        it.id,
                                    ).put(
                                        "backend",
                                        it.backend,
                                    ).put(
                                        "inbox",
                                        it.inboxId ?: "",
                                    ).put(
                                        "identity",
                                        it.identity ?: "",
                                    ).put(
                                        "private",
                                        it.allowPrivateNetwork,
                                    ),
                            )
                        }
                }.toString(),
        )
        store.edit { prefs ->
            prefs
                .asMap()
                .keys
                .filter {
                    it.name
                        .startsWith("$id/")
                }.forEach {
                    prefs
                        .remove(it)
                }
            if (prefs[stringPreferencesKey("active")] ==
                id
            ) {
                prefs
                    .remove(stringPreferencesKey("active"))
            }
        }
    }
}
