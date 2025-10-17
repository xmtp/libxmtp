package org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiArchiveOptions
import uniffi.xmtpv3.FfiBackupElementSelection
import uniffi.xmtpv3.FfiBackupMetadata

data class ArchiveOptions(
    val startNs: Long? = null,
    val endNs: Long? = null,
    val archiveElements: List<ArchiveElement> = listOf(ArchiveElement.MESSAGES, ArchiveElement.CONSENT),
)

fun ArchiveOptions.toFfi(): FfiArchiveOptions =
    FfiArchiveOptions(
        startNs = this.startNs,
        endNs = this.endNs,
        elements = this.archiveElements.map { it.toFfi() },
    )

enum class ArchiveElement {
    MESSAGES,
    CONSENT,
    ;

    fun toFfi(): FfiBackupElementSelection =
        when (this) {
            MESSAGES -> FfiBackupElementSelection.MESSAGES
            CONSENT -> FfiBackupElementSelection.CONSENT
        }

    companion object {
        fun fromFfi(ffiElement: FfiBackupElementSelection): ArchiveElement =
            when (ffiElement) {
                FfiBackupElementSelection.MESSAGES -> MESSAGES
                FfiBackupElementSelection.CONSENT -> CONSENT
            }
    }
}

data class ArchiveMetadata(
    private val ffiBackupMetadata: FfiBackupMetadata,
) {
    val archiveVersion: UShort get() = ffiBackupMetadata.backupVersion
    val elements: List<ArchiveElement>
        get() =
            ffiBackupMetadata.elements.map {
                ArchiveElement.fromFfi(
                    it,
                )
            }
    val exportedAtNs: Long get() = ffiBackupMetadata.exportedAtNs
    val startNs: Long? get() = ffiBackupMetadata.startNs
    val endNs: Long? get() = ffiBackupMetadata.endNs
}
