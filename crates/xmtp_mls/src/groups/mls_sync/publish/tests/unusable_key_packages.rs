//! A sender that fetches a key package whose addressing extensions no sender
//! can use records that installation as failed and adds the rest of its
//! inbox. The backend refuses such a package at upload, so a backend double
//! serves it, as a backend that stored it before that check would.

use super::faults::{FaultyApi, faulty_group};
use super::*;
use crate::utils::key_package_with_extensions;
use openmls::prelude::{Extension, UnknownExtension};
use xmtp_configuration::{
    WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID, WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
    WELCOME_WRAPPER_XWING_KEY_BYTES,
};
use xmtp_id::key_package::WrapperAlgorithm;
use xmtp_mls_validation::commit::extract_group_membership;
use xmtp_proto::xmtp::mls::message_contents::WelcomeWrapperEncryption;

/// A pointee AEAD extension that does not decode.
fn undecodable_pointee_aead() -> Extension {
    Extension::Unknown(
        WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID,
        UnknownExtension(vec![0xff]),
    )
}

/// An X-Wing wrapper key one byte short. It decodes, so a sender that does not
/// check its length adds the installation and then fails every Welcome of the
/// commit.
fn short_xwing_wrapper_key() -> Extension {
    let wrapper = WelcomeWrapperEncryption {
        pub_key: vec![9; WELCOME_WRAPPER_XWING_KEY_BYTES - 1],
        algorithm: WrapperAlgorithm::XWingMLKEM768Draft6.into(),
    };
    Extension::Unknown(
        WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
        UnknownExtension(wrapper.encode_to_vec()),
    )
}

/// Alix adds Bo and Caro while the backend serves Bo's second installation a
/// package carrying `extension`. The add succeeds, that installation is
/// recorded as failed, the one membership intent finishes with its Welcomes
/// sent, and Bo's first installation and Caro join.
// verifies: JOIN-090, JOIN-093, JOIN-012
#[rstest::rstest]
#[case::undecodable_pointee_aead(undecodable_pointee_aead())]
#[case::short_xwing_wrapper_key(short_xwing_wrapper_key())]
#[xmtp_common::test]
async fn unusable_package_fails_only_its_installation(#[case] extension: Extension) {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(bo2, from: bo, disable_workers);
    tester!(caro, disable_workers);
    let unusable = bo2.context.installation_id().to_vec();
    let api = FaultyApi::new(&alix, |_| false, []);
    api.served_key_packages.lock().insert(
        unusable.clone(),
        key_package_with_extensions(&bo2.context, vec![extension]),
    );
    let group_id = alix.create_group(None, None).unwrap().group_id;
    let group = faulty_group(&alix, &group_id, api).await;

    group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await
        .unwrap();

    let membership = group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>(extract_group_membership(mls_group.extensions())?)
        })
        .await
        .unwrap();
    assert_eq!(membership.failed_installations, vec![unusable]);
    let membership_intent_states: Vec<IntentState> = group
        .context
        .db()
        .find_group_intents(
            group_id,
            None,
            Some(vec![IntentKind::UpdateGroupMembership]),
        )
        .unwrap()
        .into_iter()
        .map(|intent| intent.state)
        .collect();
    assert_eq!(membership_intent_states, vec![IntentState::Processed]);
    for joiner in [&bo, &caro] {
        joiner.sync_welcomes().await.unwrap();
        assert!(joiner.group(&group_id).is_ok());
    }
}
