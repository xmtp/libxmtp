//! Identity reads through a test client.

use crate::Client;
use crate::context::XmtpSharedContext;
use crate::identity_updates::{IdentityUpdates, load_identity_updates};
use xmtp_id::associations::AssociationState;

/// The association state of `inbox_id` after `client` loads its updates.
pub async fn get_association_state<Context>(
    client: &Client<Context>,
    inbox_id: &str,
) -> AssociationState
where
    Context: XmtpSharedContext,
{
    let conn = client.context.db();
    load_identity_updates(client.context.api(), &conn, &[inbox_id])
        .await
        .unwrap();

    IdentityUpdates::new(&client.context)
        .get_association_state(&conn, inbox_id, None)
        .await
        .unwrap()
}
