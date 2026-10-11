package org.xmtp.android.example.messenger.data.profile

import android.content.Context
import org.xmtp.android.example.messenger.MessengerPreferences
import org.xmtp.android.example.messenger.ResetCleanup
import org.xmtp.android.example.messenger.ResetPhase
import org.xmtp.android.example.messenger.ResetRecord
import org.xmtp.android.example.messenger.attachments.AttachmentFiles
import org.xmtp.android.example.messenger.attachments.ExportCleanupReplay
import org.xmtp.android.example.messenger.session.SessionLifecycle

/** AppSession holds its operation lock for each journal transition. */
internal class ProfileCleanup(
    context: Context,
    private val preferences: MessengerPreferences,
    private val lifecycle: SessionLifecycle,
) {
    private val context = context.applicationContext
    private val reset = ResetCleanup(this.context.filesDir)

    fun revokeExports(profileId: String) = AttachmentFiles.revokeProfile(context, profileId)

    fun clearExports(profileId: String) = AttachmentFiles.removeProfileExports(context, profileId)

    suspend fun replayExports() =
        ExportCleanupReplay(
            preferences::pendingExportCleanup,
            ::clearExports,
            preferences::completeExportCleanup,
        ).run()

    suspend fun finishExports(profileId: String) {
        clearExports(profileId)
        preferences.completeExportCleanup(profileId)
    }

    suspend fun recoverReset(
        initial: ResetRecord,
        noOwner: Boolean,
    ) {
        check(noOwner) { "Cannot reset an open profile" }
        var record = initial
        if (record.phase == ResetPhase.STOPPING) {
            reset.removeDatabase(record, noOwner = true)
            record = record.copy(phase = ResetPhase.DATABASE_REMOVED)
            preferences.saveReset(record)
        }
        if (record.phase == ResetPhase.DATABASE_REMOVED) {
            revokeExports(record.profileId)
            reset.removeFiles(record)
            record = record.copy(phase = ResetPhase.FILES_REMOVED)
            preferences.saveReset(record)
        }
        lifecycle.beforeProfileRemoval(record.profileId)
        preferences.removeProfile(record.profileId)
        preferences.saveReset(null)
    }
}
