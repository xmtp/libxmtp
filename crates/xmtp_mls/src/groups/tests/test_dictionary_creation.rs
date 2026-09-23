use crate::{groups::app_data::committed_floor_exceeding, tester};
use openmls::{extensions::ExtensionType, messages::proposals::ProposalType};
use xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION;
use xmtp_mls_common::app_data::component_id::ComponentId;

// verifies: META-002
#[xmtp_common::test(unwrap_try = true)]
async fn test_group_context_shape_at_creation() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    let dm = crate::groups::MlsGroup::create_dm_and_insert(
        &alix.context,
        xmtp_db::group::GroupMembershipState::Allowed,
        bo.inbox_id().to_string(),
        Default::default(),
        None,
    )?;
    for conversation in [&group, &dm] {
        conversation.with_group_snapshot(|mls_group| {
            let extensions = mls_group.extensions();
            let mut actual: Vec<_> = extensions.iter().map(|ext| ext.extension_type()).collect();
            actual.sort();
            let mut expected = vec![
                ExtensionType::AppDataDictionary,
                ExtensionType::RequiredCapabilities,
            ];
            expected.sort();
            assert_eq!(actual, expected);
            let required = extensions.required_capabilities().unwrap();
            let mut actual = required.extension_types().to_vec();
            actual.sort();
            let mut expected = vec![
                ExtensionType::AppDataDictionary,
                ExtensionType::LastResort,
                ExtensionType::ApplicationId,
            ];
            expected.sort();
            assert_eq!(actual, expected);
            assert_eq!(required.proposal_types(), &[ProposalType::AppDataUpdate]);
            let dictionary = extensions.app_data_dictionary().unwrap().dictionary();
            assert_eq!(
                dictionary.get(&ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16()),
                Some(PROPOSALS_MIN_PROTOCOL_VERSION.as_bytes()),
            );
            assert_eq!(
                committed_floor_exceeding(
                    mls_group,
                    &xmtp_mls_common::libxmtp_version::LibXMTPVersion::parse(env!(
                        "CARGO_PKG_VERSION"
                    ))?,
                ),
                None,
            );
            assert_eq!(
                committed_floor_exceeding(
                    mls_group,
                    &xmtp_mls_common::libxmtp_version::LibXMTPVersion::parse("999.0.0")?,
                ),
                None,
            );
            Ok(())
        })?;
        assert!(conversation.paused_for_version()?.is_none());
    }
}
