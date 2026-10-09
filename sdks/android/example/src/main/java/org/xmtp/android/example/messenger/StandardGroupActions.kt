package org.xmtp.android.example.messenger

import uniffi.xmtp_sdk.*

/** The preset changes only built-in policies. Each write commits separately. */
suspend fun applyStandardPreset(group: Group, adminOnly: Boolean) {
    val member = if (adminOnly) PermissionPolicy.ADMIN else PermissionPolicy.ALLOW
    group.updatePermission(PermissionUpdateKind.ADD_MEMBER, member, null)
    group.updatePermission(PermissionUpdateKind.REMOVE_MEMBER, PermissionPolicy.ADMIN, null)
    group.updatePermission(PermissionUpdateKind.ADD_ADMIN, PermissionPolicy.SUPER_ADMIN, null)
    group.updatePermission(PermissionUpdateKind.REMOVE_ADMIN, PermissionPolicy.SUPER_ADMIN, null)
    for (field in listOf(MetadataFieldKind.NAME, MetadataFieldKind.DESCRIPTION, MetadataFieldKind.IMAGE_URL, MetadataFieldKind.APP_DATA)) group.updatePermission(PermissionUpdateKind.UPDATE_METADATA, member, field)
    group.updatePermission(PermissionUpdateKind.UPDATE_METADATA, PermissionPolicy.ADMIN, MetadataFieldKind.DISAPPEARING)
}
