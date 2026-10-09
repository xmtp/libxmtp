package org.xmtp.android.example.messenger
import uniffi.xmtp_sdk.*

/** The preset changes only built-in policies
. Each write commits separately. */
suspend fun applyStandardPreset(
    group: Group,
    adminOnly: Boolean,
    beforeWrite: () -> Unit = {},
) = standardPresetWrites(
    adminOnly,
    { kind, policy, field ->
        beforeWrite()
        group.updatePermission(kind, policy, field)
    },
)

internal suspend fun standardPresetWrites(
    adminOnly: Boolean,
    write: suspend (
        PermissionUpdateKind,
        PermissionPolicy,
        MetadataFieldKind?,
    ) -> Unit,
) {
    val member =
        if (adminOnly) {
            PermissionPolicy.ADMIN
        } else {
            PermissionPolicy.ALLOW
        }
    write(
        PermissionUpdateKind.ADD_MEMBER,
        member,
        null,
    )
    write(
        PermissionUpdateKind.REMOVE_MEMBER,
        PermissionPolicy.ADMIN,
        null,
    )
    write(
        PermissionUpdateKind.ADD_ADMIN,
        PermissionPolicy.SUPER_ADMIN,
        null,
    )
    write(
        PermissionUpdateKind.REMOVE_ADMIN,
        PermissionPolicy.SUPER_ADMIN,
        null,
    )
    for (field in listOf(
        MetadataFieldKind.NAME,
        MetadataFieldKind.DESCRIPTION,
        MetadataFieldKind.IMAGE_URL,
        MetadataFieldKind.APP_DATA,
    )) {
        write(
            PermissionUpdateKind.UPDATE_METADATA,
            member,
            field,
        )
    }
    write(
        PermissionUpdateKind.UPDATE_METADATA,
        PermissionPolicy.ADMIN,
        MetadataFieldKind.DISAPPEARING,
    )
}
