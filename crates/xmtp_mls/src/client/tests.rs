use crate::client::Client;
use crate::context::XmtpSharedContext;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::identity::IdentityError;
use crate::subscriptions::StreamMessages;
use crate::tester;
use crate::utils::{LocalTester, LocalTesterBuilder, Tester};
use crate::{builder::ClientBuilder, identity::serialize_key_package_hash_ref};
use diesel::RunQueryDsl;
use futures::TryStreamExt;
use futures::stream::StreamExt;
use prost::Message;
use std::time::Duration;
use xmtp_common::NS_IN_SEC;
use xmtp_common::time::now_ns;
#[cfg(not(target_arch = "wasm32"))]
use xmtp_common::toxiproxy_test;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::text::TextCodec;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::consent_record::{ConsentType, StoredConsentRecord};
use xmtp_db::identity::StoredIdentity;
use xmtp_db::prelude::*;
use xmtp_db::{
    ConnectionExt, Fetch, consent_record::ConsentState, group::GroupQueryArgs,
    group_message::MsgQueryArgs, schema::identity_updates,
};
use xmtp_id::associations::test_utils::WalletTestExt;

async fn get_key_package_init_key<Context: XmtpSharedContext, Id: AsRef<[u8]>>(
    client: &Client<Context>,
    installation_id: Id,
) -> Result<Vec<u8>, IdentityError> {
    let mut kps_map = client
        .get_key_packages_for_installation_ids(vec![installation_id.as_ref().to_vec()])
        .await
        .map_err(|_| IdentityError::NewIdentity("Failed to fetch key packages".to_string()))?;

    let kp_result = kps_map.remove(installation_id.as_ref()).ok_or_else(|| {
        IdentityError::NewIdentity(format!(
            "Missing key package for {}",
            hex::encode(installation_id.as_ref())
        ))
    })??;

    serialize_key_package_hash_ref(&kp_result.inner, &client.context.mls_provider())
}

// ============================================================
// Client::close coordinated-shutdown tests
// ============================================================

mod groups;
mod identity;
mod lifecycle;
mod sync;
