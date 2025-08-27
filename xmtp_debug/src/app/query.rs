use crate::args;
use color_eyre::eyre::Result;
use std::{collections::HashSet, sync::Arc};
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
        use openmls::prelude::OpenMlsProvider;
        tracing::info!("Fetching key packages");
        let client = crate::app::clients::temp_client(&self.network, None).await?;

        let installation_keys = opts
            .installation_keys
            .iter()
            .map(|x| hex::decode(x).map_err(Into::into))
            .collect::<Result<HashSet<_>>>()?;

        let res = client
            .context
            .sync_api()
            .api_client
            .fetch_key_packages(xmtp_proto::xmtp::mls::api::v1::FetchKeyPackagesRequest {
                installation_keys: installation_keys.iter().cloned().collect(),
            })
            .await?;
        for package in res.key_packages {
            let verified = xmtp_mls::verified_key_package_v2::VerifiedKeyPackageV2::from_bytes(
                client.context.mls_provider().crypto(),
                package.key_package_tls_serialized.as_slice(),
            )?;
            let installation_id = verified.installation_id();
            let is_verified = installation_keys.contains(&installation_id);
            let wrapper_encryption = verified
                .wrapper_encryption()
                .ok()
                .flatten()
                .map(|e| e.algorithm);

            let lifetime = verified.life_time().unwrap();
            let not_before = lifetime.not_before;
            let not_before_date = chrono::DateTime::from_timestamp(not_before as i64, 0).unwrap();
            let not_after = lifetime.not_after;
            let not_after_date = chrono::DateTime::from_timestamp(not_after as i64, 0).unwrap();
            let last_resort = verified.inner.last_resort();

            println!("  installation_id: {}", hex::encode(installation_id));
            println!("    verified: {is_verified}");
            println!(
                "    wrapper_encryption: {:?}",
                wrapper_encryption
                    .map(|e| format!("{e:?}"))
                    .unwrap_or_else(|| "Unknown".into())
            );
            println!("    not_before: {not_before_date}");
            println!("    not_after: {not_after_date}");
            println!("    last_resort: {last_resort}");
            println!("    inbox_id: {}", verified.credential.inbox_id);
            println!(
                "    hpke_init_key: {}",
                hex::encode(verified.hpke_init_key())
            );
        }
        Ok(())
    }

    pub async fn batch_query_commit_log(&self, opts: &args::BatchQueryCommitLog) -> Result<()> {
        use prost::Message;
        tracing::info!("Batch querying commit log");
        let client = crate::app::clients::temp_client(&self.network, None).await?;

        let requests = opts
            .group_ids
            .iter()
            .map(|x| {
                Ok(xmtp_proto::xmtp::mls::api::v1::QueryCommitLogRequest {
                    group_id: hex::decode(x).map_err(color_eyre::eyre::Report::from)?,
                    paging_info: Some(xmtp_proto::xmtp::mls::api::v1::PagingInfo {
                        direction: xmtp_proto::xmtp::mls::api::v1::SortDirection::Ascending as i32,
                        limit: 100,
                        id_cursor: 0,
                    }),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let res = client
            .context
            .sync_api()
            .api_client
            .query_commit_log(xmtp_proto::xmtp::mls::api::v1::BatchQueryCommitLogRequest {
                requests,
            })
            .await?;
        for response in res.responses {
            println!(
                "  group_id: {}, commits: {}",
                hex::encode(response.group_id),
                response.commit_log_entries.len()
            );
            for commit in response.commit_log_entries {
                let entry =
                    xmtp_proto::xmtp::mls::message_contents::PlaintextCommitLogEntry::decode(
                        commit.serialized_commit_log_entry.as_slice(),
                    )?;
                let commit_result =
                    xmtp_proto::xmtp::mls::message_contents::CommitResult::try_from(
                        entry.commit_result,
                    )
                    .unwrap_or(xmtp_proto::xmtp::mls::message_contents::CommitResult::Unspecified);
                if opts.skip_unspecified
                    && commit_result
                        == xmtp_proto::xmtp::mls::message_contents::CommitResult::Unspecified
                {
                    continue;
                }
                println!("    sequence_id: {}", commit.sequence_id);
                println!("      commit_sequence_id: {}", entry.commit_sequence_id);
                println!(
                    "      last_epoch_authenticator: {}",
                    hex::encode(entry.last_epoch_authenticator)
                );

                println!("      commit_result: {commit_result:?}");
                println!("      applied_epoch_number: {}", entry.applied_epoch_number);
                println!(
                    "      applied_epoch_authenticator: {}",
                    hex::encode(entry.applied_epoch_authenticator)
                );
            }
        }
        Ok(())
    }
}
