package org.xmtp.android.example.messenger

import uniffi.xmtp_sdk.GroupPolicyType
import uniffi.xmtp_sdk.MembershipState

fun policyLabel(type: GroupPolicyType?): String =
    when (type) {
        GroupPolicyType.ALL_MEMBERS -> "All members"
        GroupPolicyType.ADMIN_ONLY -> "Admins only"
        GroupPolicyType.CUSTOM -> "Custom"
        null -> ""
    }

fun membershipLabel(state: MembershipState?): String =
    when (state) {
        MembershipState.ALLOWED -> {
            "Allowed"
        }

        MembershipState.PENDING_REMOVE -> {
            "PendingRemove"
        }

        null -> {
            ""
        }

        else -> {
            state.name
                .lowercase()
                .replace('_', ' ')
                .replaceFirstChar(Char::titlecase)
        }
    }
