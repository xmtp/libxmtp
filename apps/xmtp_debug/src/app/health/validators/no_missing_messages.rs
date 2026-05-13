//! Validator: every client must have every message every other client has,
//! filtered by the join-time floor (messages sent before a client joined a
//! group are not expected to be present on that client).

use crate::app::health::context::HealthContext;
use crate::app::health::result::{OpResult, Status};
use crate::app::health::validators::Validator;
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::collections::HashSet;
use std::time::Instant;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::prelude::QueryGroupMessage;
use xmtp_proto::types::GroupId;

pub struct NoMissingMessages;

#[async_trait]
impl Validator for NoMissingMessages {
    fn name(&self) -> &'static str {
        "NoMissingMessages"
    }

    async fn validate(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let mut out = Vec::new();

        let mut all_groups: Vec<[u8; 16]> = ctx.existing_groups.clone();
        all_groups.extend(ctx.new_groups.iter().copied());

        let clients = ctx.all_clients();

        for gid_bytes in &all_groups {
            let group_id = GroupId::from(gid_bytes.as_slice());

            // (client, sequence_ids, earliest_ts) per member.
            let mut per_client: Vec<(String, HashSet<i64>, Option<i64>)> = Vec::new();
            for client in &clients {
                let db = client.db();
                let msgs = match db.get_group_messages(&group_id, &MsgQueryArgs::default()) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let mut seqs = HashSet::new();
                let mut earliest: Option<i64> = None;
                for m in msgs {
                    seqs.insert(m.sequence_id);
                    earliest = Some(match earliest {
                        None => m.sent_at_ns,
                        Some(e) => e.min(m.sent_at_ns),
                    });
                }
                per_client.push((client.inbox_id().to_string(), seqs, earliest));
            }

            for (i, (inbox, _own, earliest)) in per_client.iter().enumerate() {
                let mut union_others: HashSet<i64> = HashSet::new();
                for (j, (_, seqs, _)) in per_client.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    union_others.extend(seqs.iter());
                }
                let union_vec: Vec<u64> = union_others.into_iter().map(|s| s as u64).collect();

                let Some(client) = clients.iter().find(|c| c.inbox_id() == inbox.as_str()) else {
                    continue;
                };

                let start = Instant::now();
                let outcome = client.db().missing_messages(&group_id, &union_vec);
                let (status, error) = match outcome {
                    Ok(missing) => {
                        let join_floor = earliest.unwrap_or(i64::MIN);
                        let real_missing: Vec<_> = missing
                            .into_iter()
                            .filter(|m| m.sent_at_ns >= join_floor)
                            .collect();
                        if real_missing.is_empty() {
                            (Status::Pass, None)
                        } else {
                            (
                                Status::Fail,
                                Some(eyre!("{} missing messages", real_missing.len())),
                            )
                        }
                    }
                    Err(e) => (Status::Fail, Some(eyre!("{e}"))),
                };
                out.push(OpResult {
                    op_name: self.name(),
                    target: Some(format!(
                        "inbox={} group={}",
                        inbox,
                        hex::encode(gid_bytes)
                    )),
                    status,
                    duration: start.elapsed(),
                    error,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_stable() {
        assert_eq!(NoMissingMessages.name(), "NoMissingMessages");
    }
}
