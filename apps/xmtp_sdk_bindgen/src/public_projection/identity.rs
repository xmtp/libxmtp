//! Public identity rules that the projection applies to the binding.
//!
//! Plan Decision 6 allows no compatibility names, so the public `createGroup`,
//! `createDm`, `addMembers`, and `removeMembers` take inbox IDs or account
//! identities. The projection routes each call to the binding inbox method or
//! identity method, and the identity methods are not public. Decision 13 makes
//! an absent received identity `null`.

/// One public union and the two binding methods it routes to.
pub(super) struct Route {
    pub(super) owner: &'static str,
    /// The binding inbox method, which is also the public union's name.
    pub(super) method: &'static str,
    /// The binding identity method.
    pub(super) identity: &'static str,
    /// The member argument name.
    pub(super) member: &'static str,
    pub(super) list: bool,
}

pub(super) const ROUTES: &[Route] = &[
    Route {
        owner: "Conversations",
        method: "createGroup",
        identity: "createGroupWithIdentities",
        member: "members",
        list: true,
    },
    Route {
        owner: "Conversations",
        method: "createDm",
        identity: "createDmWithIdentity",
        member: "peer",
        list: false,
    },
    Route {
        owner: "Group",
        method: "addMembers",
        identity: "addMembersByIdentity",
        member: "members",
        list: true,
    },
    Route {
        owner: "Group",
        method: "removeMembers",
        identity: "removeMembersByIdentity",
        member: "members",
        list: true,
    },
];

/// Methods whose absent result is `null`, never `undefined`. An absent DM peer
/// and an unknown received creator or adder are loaded values, not missing
/// ones.
const NULLABLE_RESULTS: &[(&str, &str)] = &[
    ("Dm", "peerInboxId"),
    ("Dm", "creatorInboxId"),
    ("Dm", "addedByInboxId"),
    ("Group", "creatorInboxId"),
    ("Group", "addedByInboxId"),
];

pub(super) fn is_nullable(owner: &str, method: &str) -> bool {
    NULLABLE_RESULTS.contains(&(owner, method))
}
