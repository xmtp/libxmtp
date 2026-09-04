use std::collections::HashSet;

use crate::{
    d14n::QueryEnvelope,
    protocol::{
        Envelope, ResolutionError, ResolveDependencies, Resolved, types::RequiredDependency,
    },
};
use itertools::Itertools;
use tracing::warn;
use xmtp_common::{ExponentialBackoff, Strategy};
use xmtp_configuration::MAX_PAGE_SIZE;
use xmtp_proto::{
    api::{Client, Query, VectorClock},
    types::{Cursor, GlobalCursor, Topic},
    xmtp::xmtpv4::envelopes::OriginatorEnvelope,
};

/// try resolve d14n dependencies based on a backoff strategy
#[derive(Clone, Debug)]
pub struct NetworkBackoffResolver<ApiClient> {
    client: ApiClient,
    backoff: ExponentialBackoff,
}

pub fn network_backoff<ApiClient>(client: &ApiClient) -> NetworkBackoffResolver<&ApiClient> {
    NetworkBackoffResolver {
        client,
        backoff: ExponentialBackoff::default(),
    }
}

#[xmtp_common::async_trait]
impl<ApiClient> ResolveDependencies for NetworkBackoffResolver<ApiClient>
where
    ApiClient: Client,
{
    type ResolvedEnvelope = OriginatorEnvelope;
    /// Resolve dependencies, starting with a list of dependencies. Should try to resolve
    /// all dependents after `dependency`, if `Dependency` is missing as well.
    /// * Once resolved, these dependencies may have missing dependencies of their own.
    /// # Returns
    /// * `HashSet<Self::ResolvedEnvelope>`: The list of envelopes which were resolved.
    async fn resolve(
        &self,
        mut missing: HashSet<RequiredDependency>,
    ) -> Result<Resolved<Self::ResolvedEnvelope>, ResolutionError> {
        let mut attempts = 0;
        let time_spent = xmtp_common::time::Instant::now();
        let mut resolved = Vec::new();
        while !missing.is_empty() {
            if let Some(wait_for) = self.backoff.backoff(attempts, time_spent) {
                tracing::info!("waiting for {:?}", wait_for);
                xmtp_common::time::sleep(wait_for).await;
                attempts += 1;
            } else {
                missing.iter().for_each(|m| {
                    warn!("dropping dependency {}, could not resolve", m);
                });
                break;
            }
            let (topics, lcc) = lcc(&missing);
            let envelopes = QueryEnvelope::builder()
                .topics(topics)
                .last_seen(lcc)
                .limit(MAX_PAGE_SIZE)
                .build()?
                .query(&self.client)
                .await
                .map_err(ResolutionError::api)?
                .envelopes;
            let got = envelopes
                .iter()
                .map(|e| e.cursor())
                .collect::<Result<HashSet<Cursor>, _>>()?;
            missing.retain(|m| !got.contains(&m.cursor));
            resolved.extend(envelopes);
        }
        Ok(Resolved {
            resolved,
            unresolved: (!missing.is_empty()).then_some(missing),
        })
    }
}

/// Get the LCC and topics from a list of missing envelopes
fn lcc(missing: &HashSet<RequiredDependency>) -> (Vec<Topic>, GlobalCursor) {
    // get the lcc by first getting lowest Cursor
    // per topic, then merging the global cursor of every topic into
    // one.
    let (topics, last_seen): (Vec<_>, Vec<GlobalCursor>) = missing
        .iter()
        .into_grouping_map_by(|m| m.topic.clone())
        .fold(GlobalCursor::default(), |mut acc, _key, val| {
            acc.apply_least(&val.cursor);
            acc
        })
        .into_iter()
        .unzip();
    let last_seen = last_seen
        .into_iter()
        .fold(GlobalCursor::default(), |mut acc, clock| {
            acc.merge_least(&clock);
            acc
        });
    (topics, last_seen)
}
