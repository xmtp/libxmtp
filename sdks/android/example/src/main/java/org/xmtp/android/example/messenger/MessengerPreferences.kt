package org.xmtp.android.example.messenger

import android.content.Context
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.json.JSONArray
import org.json.JSONObject
import org.xmtp.android.example.shared.ScrollAnchor

private val Context.messengerDataStore by preferencesDataStore("messenger")

/** Small references only. Message bodies stay in SDK storage. */
class MessengerPreferences(context: Context) {
    private val draftLock = Mutex()
    private val store = context.applicationContext.messengerDataStore
    private suspend fun get(key: String) = store.data.first()[stringPreferencesKey(key)]
    private suspend fun put(key: String, value: String?) { store.edit { if (value == null) it.remove(stringPreferencesKey(key)) else it[stringPreferencesKey(key)] = value } }
    suspend fun profiles(): List<BackendProfile> {
        val array = JSONArray(get("profiles") ?: "[]")
        return (0 until array.length()).map { i -> array.getJSONObject(i).let { BackendProfile(it.getString("id"), it.getString("backend"), it.optString("inbox").takeIf(String::isNotEmpty), it.optString("identity").takeIf(String::isNotEmpty), it.optBoolean("private")) } }
    }
    suspend fun saveProfile(profile: BackendProfile) {
        val values = profiles().filterNot { it.id == profile.id } + profile
        put("profiles", JSONArray().apply { values.forEach { put(JSONObject().put("id", it.id).put("backend", it.backend).put("inbox", it.inboxId ?: "").put("identity", it.identity ?: "").put("private", it.allowPrivateNetwork)) } }.toString())
    }
    suspend fun active(): BackendProfile? = profiles().firstOrNull { it.id == get("active") }
    suspend fun setActive(profile: BackendProfile) { saveProfile(profile); put("active", profile.id) }
    suspend fun signedIn() = get("signed-in") == "true"
    suspend fun setSignedIn(value: Boolean) = put("signed-in", value.toString())
    suspend fun reset(): ResetRecord? = get("reset")?.let { JSONObject(it).let { o -> ResetRecord(o.getString("profile"), o.getString("db"), o.getJSONArray("files").let { a -> (0 until a.length()).map(a::getString) }, ResetPhase.valueOf(o.getString("phase"))) } }
    suspend fun saveReset(value: ResetRecord?) = put("reset", value?.let { JSONObject().put("profile", it.profileId).put("db", it.ownedDatabasePath).put("files", JSONArray(it.ownedFiles)).put("phase", it.phase.name).toString() })
    suspend fun marker(profile: String, conversation: String) = ReadMarker(get("$profile/read/$conversation")?.toLong())
    suspend fun saveMarker(profile: String, conversation: String, value: Long) = put("$profile/read/$conversation", value.toString())
    suspend fun anchor(profile: String, conversation: String): ScrollAnchor? = get("$profile/scroll/$conversation")?.let { JSONObject(it).let { o -> ScrollAnchor(o.getString("id"), o.getLong("ns"), o.getInt("px"), o.getBoolean("newest")) } }
    suspend fun saveAnchor(profile: String, conversation: String, value: ScrollAnchor) = put("$profile/scroll/$conversation", JSONObject().put("id", value.messageId).put("ns", value.sentAtNs).put("px", value.offsetPx).put("newest", value.wasAtNewest).toString())
    suspend fun drafts(profile: String): List<SendDraftRef> {
        val array = JSONArray(get("$profile/drafts") ?: "[]")
        return (0 until array.length()).map { i -> array.getJSONObject(i).let { SendDraftRef(it.getString("id"), it.getString("conversation"), it.optString("secret").takeIf(String::isNotEmpty), it.optString("accepted").takeIf(String::isNotEmpty), SendPhase.valueOf(it.getString("phase"))) } }
    }
    suspend fun saveDraft(profile: String, draft: SendDraftRef) = draftLock.withLock { writeDrafts(profile, drafts(profile).filterNot { it.draftId == draft.draftId } + draft) }
    suspend fun removeDraft(profile: String, id: String) = draftLock.withLock { writeDrafts(profile, drafts(profile).filterNot { it.draftId == id }) }
    private suspend fun writeDrafts(profile: String, values: List<SendDraftRef>) = put("$profile/drafts", JSONArray().apply { values.forEach { put(JSONObject().put("id", it.draftId).put("conversation", it.conversationKey).put("secret", it.descriptorSecretRef ?: "").put("accepted", it.acceptedMessageId ?: "").put("phase", it.phase.name)) } }.toString())
    suspend fun removeProfile(id: String) {
        put("profiles", JSONArray().apply { profiles().filterNot { it.id == id }.forEach { put(JSONObject().put("id", it.id).put("backend", it.backend).put("inbox", it.inboxId ?: "").put("identity", it.identity ?: "").put("private", it.allowPrivateNetwork)) } }.toString())
        store.edit { prefs -> prefs.asMap().keys.filter { it.name.startsWith("$id/") }.forEach { prefs.remove(it) }; if (prefs[stringPreferencesKey("active")] == id) prefs.remove(stringPreferencesKey("active")) }
    }
}
