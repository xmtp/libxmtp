use crate::args;
use color_eyre::eyre::Result;
use std::sync::Arc;
use xmtp_mls::context::XmtpSharedContext;

pub struct Query {
    opts: args::Query,
    #[allow(unused)]
    network: args::BackendOpts,
    #[allow(unused)]
    store: Arc<redb::Database>,
}

impl Query {
    pub fn new(opts: args::Query, network: args::BackendOpts, store: Arc<redb::Database>) -> Self {
        Self {
            opts,
            network,
            store,
        }
    }

    pub async fn run(self) -> Result<()> {
        match &self.opts {
            args::Query::Identity(opts) => self.identity(opts).await,
            args::Query::FetchKeyPackages(opts) => self.fetch_key_packages(opts).await,
            args::Query::BatchQueryCommitLog(opts) => self.batch_query_commit_log(opts).await,
        }
    }

    pub async fn identity(&self, opts: &args::Identity) -> Result<()> {
        tracing::info!("Fetching identity for inbox: {}", opts.inbox_id);
        let client = crate::app::clients::temp_client(&self.network, None).await?;

        let res = client
            .context
            .sync_api()
            .api_client
            .get_identity_updates_v2(
                xmtp_proto::xmtp::identity::api::v1::GetIdentityUpdatesRequest {
                    requests: vec![
                xmtp_proto::xmtp::identity::api::v1::get_identity_updates_request::Request {
                  inbox_id: opts.inbox_id.to_string(),
                  sequence_id: 0,
                }
              ],
                },
            )
            .await?
            .responses;

        tracing::info!("Identity updates: {}", res.len());
        for response in res {
            let inbox_id = response.inbox_id;
            let updates = response.updates;
            println!("inbox_id: {}, updates: {}", inbox_id, updates.len());
            for update in updates {
                // dbg!(&update);
                let server_timestamp =
                    chrono::DateTime::from_timestamp_nanos(update.server_timestamp_ns as i64);
                let Some(new_update) = update.update else {
                    println!(
                        "  sequence_id: {}, server_timestamp: {server_timestamp}",
                        update.sequence_id
                    );
                    continue;
                };
                let client_timestamp =
                    chrono::DateTime::from_timestamp_nanos(new_update.client_timestamp_ns as i64);
                println!(
                    "  sequence_id: {:?}, server_timestamp: {server_timestamp}, client_timestamp: {client_timestamp}",
                    update.sequence_id
                );
                for action in new_update.actions {
                    // TODO: verify signature here
                    let Some(kind) = action.kind else {
                        println!("    action has no kind");
                        continue;
                    };
                    match kind {
                        xmtp_proto::xmtp::identity::associations::identity_action::Kind::CreateInbox(create_inbox) => {
                          println!("    create_inbox: nonce: {}, account_identifier: {}", create_inbox.nonce, create_inbox.initial_identifier);
                        }
                        xmtp_proto::xmtp::identity::associations::identity_action::Kind::Add(add_association) => {
                          let new_member_identifier = match add_association.new_member_identifier.and_then(|x| x.kind) {
                            Some(xmtp_proto::xmtp::identity::associations::member_identifier::Kind::EthereumAddress(address)) => format!("eth: {address}"),
                            Some(xmtp_proto::xmtp::identity::associations::member_identifier::Kind::InstallationPublicKey(public_key)) => format!("installation pubkey: {}", hex::encode(public_key)),
                            Some(xmtp_proto::xmtp::identity::associations::member_identifier::Kind::Passkey(passkey)) => format!("passkey: {}", hex::encode(passkey.key)),
                            None => String::new(),
                          };
                          // TODO: maybe add signatures
                          println!("    add_association: new_member_identifier: {new_member_identifier}");
                        }
                        xmtp_proto::xmtp::identity::associations::identity_action::Kind::Revoke(revoke_association) => {
                          let revoke_member_identifier = match revoke_association.member_to_revoke.and_then(|x| x.kind) {
                            Some(xmtp_proto::xmtp::identity::associations::member_identifier::Kind::EthereumAddress(address)) => format!("eth: {address}"),
                            Some(xmtp_proto::xmtp::identity::associations::member_identifier::Kind::InstallationPublicKey(public_key)) => format!("installation pubkey: {}", hex::encode(public_key)),
                            Some(xmtp_proto::xmtp::identity::associations::member_identifier::Kind::Passkey(passkey)) => format!("passkey: {}", hex::encode(passkey.key)),
                            None => String::new(),
                          };
                          println!("    revoke_association: member_to_revoke: {revoke_member_identifier}");
                        }
                        xmtp_proto::xmtp::identity::associations::identity_action::Kind::ChangeRecoveryAddress(change_recovery_address) => {
                          let new_recovery_address = change_recovery_address.new_recovery_identifier;
                          println!("    change_recovery_address: new_recovery_address: {new_recovery_address}");
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn fetch_key_packages(&self, opts: &args::FetchKeyPackages) -> Result<()> {
        tracing::info!("Fetching key packages");
        let _ = opts;
        Ok(())
    }

    pub async fn batch_query_commit_log(&self, opts: &args::BatchQueryCommitLog) -> Result<()> {
        tracing::info!("Batch querying commit log");
        let _ = opts;
        Ok(())
    }
}
