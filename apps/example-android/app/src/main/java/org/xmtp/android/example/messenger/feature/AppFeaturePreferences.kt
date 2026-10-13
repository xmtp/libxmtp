package org.xmtp.android.example.messenger.feature

import android.content.Context
import org.json.JSONObject
import org.xmtp.android.example.messenger.AppSession
import org.xmtp.android.example.messenger.SecureSecretStore
import org.xmtp.android.example.messenger.SessionKey
import org.xmtp.android.example.shared.Appearance
import java.security.MessageDigest

/** Profile values use the existing encrypted profile store and cleanup path. */
class AppFeaturePreferences(
    context: Context,
    private val session: AppSession,
    private val secrets: SecureSecretStore,
) {
    private val appearanceStore =
        context.applicationContext.getSharedPreferences(
            "messenger-appearance",
            Context.MODE_PRIVATE,
        )
    private val lock = Any()

    fun appearance(): Appearance =
        runCatching {
            Appearance.valueOf(appearanceStore.getString("appearance", "SYSTEM") ?: "SYSTEM")
        }.getOrDefault(Appearance.SYSTEM)

    fun setAppearance(value: Appearance) {
        appearanceStore.edit().putString("appearance", value.name).apply()
    }

    private fun values(profile: String) =
        secrets.read(profile, "feature-preferences")?.let { JSONObject(String(it, Charsets.UTF_8)) } ?: JSONObject()

    fun alias(
        profile: String,
        id: String,
    ): String? =
        synchronized(lock) {
            values(profile).optJSONObject("aliases")?.optString(id)?.takeIf(String::isNotBlank)
        }

    fun dismissed(
        profile: String,
        id: String,
    ): Boolean =
        synchronized(lock) {
            values(profile).optJSONObject("dismissed")?.optBoolean(id) ==
                true
        }

    fun label(profile: String): String = synchronized(lock) { values(profile).optString("server-label") }

    private fun write(
        key: SessionKey,
        change: (JSONObject) -> Unit,
    ): Boolean =
        session.admit(key) {
            synchronized(lock) {
                val data = values(key.profileId)
                change(data)
                secrets.write(key.profileId, "feature-preferences", data.toString().toByteArray(Charsets.UTF_8))
            }
        }

    fun setAlias(
        key: SessionKey,
        id: String,
        alias: String,
    ) = write(key) { data ->
        val aliases = data.optJSONObject("aliases") ?: JSONObject()
        if (alias.isBlank()) aliases.remove(id) else aliases.put(id, alias.trim())
        data.put("aliases", aliases)
    }

    fun dismiss(
        key: SessionKey,
        id: String,
    ) = write(key) { data ->
        data.put(
            "dismissed",
            (
                data.optJSONObject("dismissed")
                    ?: JSONObject()
            ).put(id, true),
        )
    }

    fun setLabel(
        key: SessionKey,
        label: String,
    ) = write(key) { it.put("server-label", label.trim()) }

    /** True after the account claimed or skipped a handle in onboarding. */
    fun handleStepDone(profile: String): Boolean = synchronized(lock) { values(profile).optBoolean("handle-step-done") }

    fun finishHandleStep(key: SessionKey) = write(key) { it.put("handle-step-done", true) }

    /** The time of each handle claim this device made, in milliseconds. Claims from other devices are absent. */
    fun handleClaims(profile: String): Map<String, Long> =
        synchronized(lock) {
            val claims = values(profile).optJSONObject("handle-claims") ?: return@synchronized emptyMap()
            claims.keys().asSequence().associateWith { claims.optLong(it) }
        }

    fun recordHandleClaim(
        key: SessionKey,
        handle: String,
        atMs: Long,
    ) = write(key) { data ->
        data.put("handle-claims", (data.optJSONObject("handle-claims") ?: JSONObject()).put(handle, atMs))
    }
}

data class ComposerDraft(
    val text: String = "",
    val revision: Long = 0,
    val replyId: String? = null,
    val requestId: Long? = null,
    val submittedRevision: Long? = null,
    val draftId: String? = null,
    val acceptedId: String? = null,
    val submittedReplyId: String? = null,
)

/** No message body enters route state or the reference DataStore. */
class SecureComposerDrafts(
    private val session: AppSession,
    private val secrets: SecureSecretStore,
) {
    private val lock = Any()

    private fun ref(id: String) =
        "composer-" +
            MessageDigest.getInstance("SHA-256").digest(id.toByteArray()).joinToString("") { "%02x".format(it) }

    fun read(
        profile: String,
        id: String,
    ): ComposerDraft =
        synchronized(lock) {
            val bytes = secrets.read(profile, ref(id)) ?: return@synchronized ComposerDraft()
            val value = JSONObject(String(bytes, Charsets.UTF_8))
            ComposerDraft(
                value.optString(
                    "text",
                ),
                value.optLong(
                    "revision",
                ),
                value.optString("reply").takeIf(String::isNotBlank),
                value.optLong("request").takeIf {
                    value.has("request")
                },
                value.optLong("submitted").takeIf {
                    value.has("submitted")
                },
                value
                    .optString(
                        "draft",
                    ).takeIf(
                        String::isNotBlank,
                    ),
                value
                    .optString(
                        "accepted",
                    ).takeIf(String::isNotBlank),
                value.optString("submitted-reply").takeIf(String::isNotBlank),
            )
        }

    fun write(
        key: SessionKey,
        id: String,
        value: ComposerDraft,
    ): Boolean =
        session.admit(key) {
            synchronized(lock) {
                val data =
                    JSONObject()
                        .put("text", value.text)
                        .put("revision", value.revision)
                        .put("reply", value.replyId)
                        .put(
                            "request",
                            value.requestId,
                        ).put(
                            "submitted",
                            value.submittedRevision,
                        ).put(
                            "draft",
                            value.draftId,
                        ).put("accepted", value.acceptedId)
                        .put("submitted-reply", value.submittedReplyId)
                secrets.write(key.profileId, ref(id), data.toString().toByteArray(Charsets.UTF_8))
            }
        }
}
