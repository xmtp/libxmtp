use super::*;
use crate::{Reaction, ReactionAction, ReactionSchema};
use xmtp_db::group_message::{QueryGroupMessage, StoredGroupMessage};
use xmtp_db::{ConnectionExt, Store, diesel::prelude::*, schema::group_messages::dsl};
use xmtp_proto::types::GroupId;

fn thumbs_up() -> Reaction {
    Reaction {
        content: "👍".into(),
        action: ReactionAction::Added,
        schema: ReactionSchema::Unicode,
    }
}

/// A copy of a stored reaction with its own id and position, in `group_id`.
fn reaction_copy(
    template: &StoredGroupMessage,
    group_id: GroupId,
    id: Vec<u8>,
    sent_at_ns: i64,
) -> StoredGroupMessage {
    StoredGroupMessage {
        idempotency_key: hex::encode(&id),
        id,
        group_id,
        sent_at_ns,
        sequence_id: 0,
        envelope_hash: None,
        ..template.clone()
    }
}

fn reaction_ids(page: &[crate::ReactionMessage]) -> Vec<MessageId> {
    page.iter().map(|reaction| reaction.id.clone()).collect()
}

/// Reaction pages run newest first by send time, then sequence id, then id,
/// so a page boundary inside a run of equal send times neither skips nor
/// repeats a reaction. A bad limit or cursor is an invalid argument, and a
/// deleted or expired parent shows no reactions, as in its summary.
// verifies: PROC-054
#[xmtp_common::test(unwrap_try = true)]
async fn message_reactions_page_newest_first() {
    const COPIES: u8 = 119;
    // Copies 30 to 59 share a send time: rows 32 to 61, across the first
    // page boundary.
    const TIES: std::ops::Range<u8> = 30..60;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let parent = group.send_text("parent".into(), None).await?;
    let other_parent = group.send_text("other parent".into(), None).await?;
    let sent = group
        .send_reaction(parent.clone(), Some(client.inbox_id()), thumbs_up(), None)
        .await?;
    let db = client.inner.context.db();
    let template = client.inner.message(sent.to_bytes()?)?;
    let base = template.sent_at_ns - 1_000_000;
    let mut stored = vec![template.clone()];
    for index in 0..COPIES {
        let rank = if TIES.contains(&index) {
            TIES.start
        } else {
            index
        };
        let copy = reaction_copy(
            &template,
            group.inner.group_id,
            vec![index; 32],
            base - i64::from(rank) * 10,
        );
        copy.store(&db)?;
        stored.push(copy);
    }
    stored.sort_by(|a, b| {
        (b.sent_at_ns, b.sequence_id, &b.id).cmp(&(a.sent_at_ns, a.sequence_id, &a.id))
    });
    let newest_first = stored
        .iter()
        .map(|row| MessageId::from_bytes(&row.id))
        .collect::<Result<Vec<_>, _>>()?;

    let mut paged = Vec::new();
    let mut before = None;
    for expected in [50, 50, 20] {
        let page = group
            .message_reactions(parent.clone(), before.clone(), None)
            .await?;
        assert_eq!(page.len(), expected);
        before = page.last().map(|reaction| reaction.id.clone());
        paged.extend(reaction_ids(&page));
    }
    assert!(
        group
            .message_reactions(parent.clone(), before, None)
            .await?
            .is_empty()
    );
    assert_eq!(paged, newest_first);

    let mut elsewhere = reaction_copy(&template, group.inner.group_id, vec![0xe0; 32], base);
    elsewhere.reference_id = Some(other_parent.to_bytes()?);
    elsewhere.store(&db)?;
    let mut expired = reaction_copy(&template, group.inner.group_id, vec![0xe1; 32], base);
    expired.expire_at_ns = Some(xmtp_common::time::now_ns() - 1);
    expired.store(&db)?;
    let reply = group
        .send_reply(
            parent.clone(),
            Some(client.inbox_id()),
            crate::encode_text("reply".into())?,
            None,
        )
        .await?;
    for (before, limit) in [
        (None, Some(0)),
        (None, Some(1001)),
        (Some(MessageId::from_bytes(&elsewhere.id)?), None),
        (Some(MessageId::from_bytes(&expired.id)?), None),
        // A reply to the same parent is not a reaction.
        (Some(reply), None),
    ] {
        let result = group
            .message_reactions(parent.clone(), before.clone(), limit)
            .await;
        assert!(
            matches!(result, Err(XmtpError::InvalidArgument(_))),
            "before {before:?} with limit {limit:?} was accepted"
        );
    }

    let other_reactions = || group.message_reactions(other_parent.clone(), None, None);
    assert_eq!(other_reactions().await?.len(), 1);
    let other_id = other_parent.to_bytes()?;
    db.raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(&other_id)))
            .set(dsl::expire_at_ns.eq(xmtp_common::time::now_ns() - 1))
            .execute(conn)
    })?;
    assert!(other_reactions().await?.is_empty());
    group.delete_message(parent.clone()).await?;
    assert!(
        group
            .message_reactions(parent.clone(), None, None)
            .await?
            .is_empty()
    );
    client.end().await?;
}

/// A stitched DM's reactions can be stored in either of its groups. The
/// summary, the count and the pages range over both, through either group.
// verifies: PROC-053, PROC-054, DMS-009
#[xmtp_common::test(unwrap_try = true)]
async fn message_reactions_span_stitched_dm_groups() {
    const PER_GROUP: u8 = 30;

    let a = Client::create(crate::generate_local_signer().await, options()).await?;
    let b = Client::create(crate::generate_local_signer().await, options()).await?;
    let first = a.conversations().create_dm(b.inbox_id(), None).await?;
    let second = b.conversations().create_dm(a.inbox_id(), None).await?;
    assert_ne!(first.inner.group_id, second.inner.group_id);
    let parent = first.send_text("parent".into(), None).await?;
    let sent = first
        .send_reaction(parent.clone(), Some(a.inbox_id()), thumbs_up(), None)
        .await?;
    a.conversations().sync_all(None).await?;
    // The sibling group itself, not the DM's listed group.
    let sibling = a.inner.group(&second.inner.group_id)?;
    let Some(crate::Conversation::Dm { dm: a_second }) =
        crate::Conversation::from_core(sibling, a.key).await?
    else {
        panic!("sibling is a DM");
    };

    // The first group's reactions are the newest, so the first page read
    // through it ends on a reaction stored in the sibling group.
    let db = a.inner.context.db();
    let template = a.inner.message(sent.to_bytes()?)?;
    db.delete_message_by_id(&template.id)?;
    let base = template.sent_at_ns;
    for (group_id, offset, marker) in [
        (first.inner.group_id, 1_000, 0x10),
        (second.inner.group_id, 0, 0x40),
    ] {
        for index in 0..PER_GROUP {
            reaction_copy(
                &template,
                group_id,
                vec![marker + index; 32],
                base + offset + i64::from(index),
            )
            .store(&db)?;
        }
    }

    for dm in [&first, &a_second] {
        let history = dm.messages(None).await?;
        let summary = history
            .iter()
            .find(|message| message.0.id == parent)
            .expect("parent in the stitched history");
        assert_eq!(summary.0.reactions.len(), 50);
        assert_eq!(summary.0.reaction_message_count, u64::from(PER_GROUP) * 2);
    }

    let page = first.message_reactions(parent.clone(), None, None).await?;
    assert_eq!(page.len(), 50);
    let last = page.last().expect("a full page").id.to_bytes()?;
    assert_eq!(
        a.inner.message(last)?.group_id,
        second.inner.group_id,
        "the first page ends in the sibling group"
    );
    let rest = first
        .message_reactions(parent.clone(), page.last().map(|r| r.id.clone()), None)
        .await?;
    assert_eq!(rest.len(), 10);
    let seen: std::collections::HashSet<_> = reaction_ids(&page)
        .into_iter()
        .chain(reaction_ids(&rest))
        .collect();
    assert_eq!(seen.len(), usize::from(PER_GROUP) * 2);
    b.end().await?;
    a.end().await?;
}
