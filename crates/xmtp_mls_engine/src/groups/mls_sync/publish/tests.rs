use super::*;
use crate::groups::test_support::{EnginePeer, EngineSuiteOps, EngineTestPeer, EngineTestPeerOps};
use crate::prelude::*;
use prost::Message;
use xmtp_mls_validation::commit::CommitRuleError;
use xmtp_proto::types::Topic;

mod cancel;
mod deadlines;
mod dictionary_creation;
mod faults;
mod membership_component;
mod membership_recovery;
mod out_of_range_settlement;
mod plaintext;
mod rate_limits;
mod removal_selection;
mod send_state_transitions;
mod signing;
mod split_refresh;
mod stale_refresh;
mod uncommittable_proposals;
mod unusable_key_packages;

mod transcript;

mod attempt_storage;
mod prepared_publish;
mod rejected_intents;
mod welcome_followups;

async fn prepare_message<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
) -> Result<(StoredGroupIntent, PreparedAttempt), GroupError> {
    prepare_kind(group, IntentKind::SendMessage).await
}

async fn prepare_kind<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    kind: IntentKind,
) -> Result<(StoredGroupIntent, PreparedAttempt), GroupError> {
    let requirements = xmtp_mls_core::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, storage| {
            let intent = storage
                .db()
                .find_group_intents(
                    group.group_id,
                    Some(vec![IntentState::ToPublish]),
                    Some(vec![kind]),
                )?
                .into_iter()
                .next()
                .ok_or(GroupError::UninitializedResult)?;
            PublishRequirements::capture(mls_group, &intent).map(Continue)
        })
    })?
    .into_continued();
    let mut dependencies = group.resolve_publish_dependencies(&requirements).await?;
    let attempt = group
        .prepare_publish_attempt(&requirements, &mut dependencies)?
        .ok_or(GroupError::UninitializedResult)?;
    Ok((requirements.intent, attempt))
}

/// Publish a membership change that adds `inbox_ids` and apply its echo,
/// without sending its Welcomes. Returns the committed intent's id.
async fn commit_membership_change<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    inbox_ids: &[&str],
) -> i32 {
    let data = group
        .get_membership_update_intent(inbox_ids, &[])
        .await
        .unwrap();
    let intent = QueueIntent::update_group_membership()
        .data(data)
        .queue(group)
        .unwrap();
    group.publish_intents().await.unwrap();
    assert!(!group.receive().await.unwrap().is_errored());
    intent.id
}

/// The call reports that the membership change is applied but its Welcomes
/// were not sent.
fn welcomes_unsendable<T>(result: &Result<T, GroupError>) -> bool {
    matches!(
        result,
        Err(GroupError::OutgoingPreparation(
            OutgoingPreparationError::WelcomesUnsendable
        ))
    )
}

/// A frozen stored attempt: its file name, its hex text, and the SHA-256 of
/// the text.
struct GoldenAttempt {
    name: &'static str,
    text: &'static str,
    sha256: &'static str,
}

/// Stored attempts with every optional field set: receipts, a Welcome batch
/// with receipts, a rejection, an unsettled batch, a standalone proposal, and
/// a proposal reference in the base. Every name in them, nested ones included, is stored in rows of
/// in-flight intents, so each file is frozen; the newest is the format this
/// build writes. For a deliberate format change, run
/// `dev/nix-shell 'just test workspace -p xmtp_mls write_golden_prepared_attempt --run-ignored only'`,
/// rename `fixtures/prepared_attempt_next.cbor.hex` to
/// `prepared_attempt_<release>.cbor.hex`, and add it last here with its hash.
const GOLDEN_PREPARED_ATTEMPTS: &[GoldenAttempt] = &[GoldenAttempt {
    name: "prepared_attempt_rc4.cbor.hex",
    text: include_str!("fixtures/prepared_attempt_rc4.cbor.hex"),
    sha256: "b4581c9dd5e748865c08f0db462f85d4165b8310a1e9e7ee6d8cbab5ca99d722",
}];

/// The names serde stores the variants of the enum `T` by, in declaration
/// order, read from its derived `Deserialize`. A renamed, added or removed
/// variant changes the list.
pub(super) fn serde_variants<T: serde::de::DeserializeOwned>() -> &'static [&'static str] {
    use serde::de::{Error, Visitor};

    #[derive(Debug)]
    struct Stop;
    impl std::fmt::Display for Stop {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("not an enum")
        }
    }
    impl std::error::Error for Stop {}
    impl Error for Stop {
        fn custom<M: std::fmt::Display>(_: M) -> Self {
            Stop
        }
    }
    struct Variants(Option<&'static [&'static str]>);
    impl<'de> serde::Deserializer<'de> for &mut Variants {
        type Error = Stop;
        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Stop> {
            Err(Stop)
        }
        fn deserialize_enum<V: Visitor<'de>>(
            self,
            _: &'static str,
            variants: &'static [&'static str],
            _: V,
        ) -> Result<V::Value, Stop> {
            self.0 = Some(variants);
            Err(Stop)
        }
        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
            byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map
            struct identifier ignored_any
        }
    }
    let mut variants = Variants(None);
    let _ = T::deserialize(&mut variants);
    variants.0.expect("an enum")
}
