package org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiGroupMembershipState

/**
 * Represents the membership state of a group member.
 *
 * This wrapper provides a Kotlin-friendly interface for the FFI group membership state.
 */
enum class GroupMembershipState {
    /**
     * Member is allowed in the group
     */
    ALLOWED,

    /**
     * Member was rejected from the group
     */
    REJECTED,

    /**
     * Member is pending approval
     */
    PENDING,

    /**
     * Member was restored to the group
     */
    RESTORED,

    /**
     * Member is pending removal from the group
     */
    PENDING_REMOVE,

    ;

    companion object {
        /**
         * Converts from FFI GroupMembershipState to Kotlin enum
         */
        fun fromFfiGroupMembershipState(ffiState: FfiGroupMembershipState): GroupMembershipState =
            when (ffiState) {
                FfiGroupMembershipState.ALLOWED -> ALLOWED
                FfiGroupMembershipState.REJECTED -> REJECTED
                FfiGroupMembershipState.PENDING -> PENDING
                FfiGroupMembershipState.RESTORED -> RESTORED
                FfiGroupMembershipState.PENDING_REMOVE -> PENDING_REMOVE
            }
    }

    /**
     * Converts this Kotlin enum to FFI GroupMembershipState
     */
    fun toFfi(): FfiGroupMembershipState =
        when (this) {
            ALLOWED -> FfiGroupMembershipState.ALLOWED
            REJECTED -> FfiGroupMembershipState.REJECTED
            PENDING -> FfiGroupMembershipState.PENDING
            RESTORED -> FfiGroupMembershipState.RESTORED
            PENDING_REMOVE -> FfiGroupMembershipState.PENDING_REMOVE
        }
}
