use crate::{
    backend_v1::EnvelopeMeta,
    xmtp::{
        identity::associations::IdentityUpdate, mls::message_contents::PlaintextCommitLogEntry,
    },
};

#[derive(Clone, Debug)]
pub struct CommitLogEntry {
    pub meta: EnvelopeMeta,
    pub entry: PlaintextCommitLogEntry,
    pub payload: crate::backend_v1::CommitLogEntry,
}
#[derive(Clone, Debug)]
pub struct IdentityUpdateLog {
    pub meta: EnvelopeMeta,
    pub update: IdentityUpdate,
}
