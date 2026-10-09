package org.xmtp.android.example.messenger

import java.io.File
import org.xmtp.android.example.shared.ScrollAnchor

data class SessionKey(val profileId: String, val generation: Long)
data class ReadMarker(val insertedAtNs: Long?)
enum class ResetPhase { STOPPING, DATABASE_REMOVED, FILES_REMOVED }
data class ResetRecord(val profileId: String, val ownedDatabasePath: String, val ownedFiles: List<String>, val phase: ResetPhase)
enum class SendPhase { DRAFT, UPLOADING, QUEUEING, ACCEPTED }
data class SendDraftRef(val draftId: String, val conversationKey: String, val descriptorSecretRef: String? = null, val acceptedMessageId: String? = null, val phase: SendPhase = SendPhase.DRAFT)
data class BackendProfile(val id: String, val backend: String, val inboxId: String? = null, val identity: String? = null, val allowPrivateNetwork: Boolean = false) {
    fun paths(filesDir: File) = ProfilePaths(File(filesDir, "messenger/profiles/$id"), File(filesDir, "messenger-exports/$id"))
}
data class ProfilePaths(val root: File, val exportRoot: File) {
    val database = File(root, "sdk/messages.db3")
    val attachments = File(root, "sdk/attachments")
    val temp = File(root, "temp")
    val exports = exportRoot
    val secrets = File(root, "secrets")
    fun resetRecord(profileId: String) = ResetRecord(profileId, database.absolutePath, listOf(root.absolutePath, exports.absolutePath), ResetPhase.STOPPING)
}

/** A generation changes before any old owner starts to stop. */
class SessionFence {
    @Volatile private var generation = 0L
    @Volatile private var profileId: String? = null
    @Synchronized fun replace(profile: String?): SessionKey? {
        generation += 1
        profileId = profile
        return profile?.let { SessionKey(it, generation) }
    }
    fun accepts(key: SessionKey) = key.profileId == profileId && key.generation == generation
    @Synchronized fun <T> withCurrent(key: SessionKey, block: () -> T): T? = if (accepts(key)) block() else null
}

data class SavedPosition(val key: String, val anchor: ScrollAnchor)
