//! Deferred admission and refresh share one client lifetime and commit lock.

use super::{
    BlockedConnection, ConfigurationFetchError, check_minimum_version, commit_configuration,
};
use crate::{
    client::ClientError,
    context::{XmtpMlsLocalContext, XmtpSharedContext},
};
use std::sync::{Arc, Weak, atomic::Ordering};
use xmtp_api::preflight::{ConfigurationFetch, PreflightError, RequestPreflight};
use xmtp_configuration::ServerConfiguration;

struct ClientPreflight<A, Db, S> {
    context: Weak<XmtpMlsLocalContext<A, Db, S>>,
}

pub(crate) fn bind_preflight<A, Db, S>(context: &Arc<XmtpMlsLocalContext<A, Db, S>>)
where
    A: xmtp_proto::api_client::XmtpApi + 'static,
    Db: xmtp_db::XmtpDb + 'static,
    S: xmtp_db::XmtpMlsStorageProvider + 'static,
{
    let weak = Arc::downgrade(context);
    assert!(
        context.api().api_client.bind_preflight(|fetch| {
            let _ = context.server_configuration().fetch.set(fetch);
            Arc::new(ClientPreflight { context: weak })
        }),
        "client preflight is bound once"
    );
}

#[xmtp_common::async_trait]
impl<A, Db, S> RequestPreflight for ClientPreflight<A, Db, S>
where
    A: xmtp_proto::api_client::XmtpApi + 'static,
    Db: xmtp_db::XmtpDb + 'static,
    S: xmtp_db::XmtpMlsStorageProvider + 'static,
{
    async fn check(&self, fetch: &dyn ConfigurationFetch) -> Result<(), PreflightError> {
        let context = self
            .context
            .upgrade()
            .ok_or_else(|| PreflightError::new(ClientError::AlreadyClosed))?;
        run(&context, fetch, false)
            .await
            .map(|_| ())
            .map_err(PreflightError::new)
    }
}

fn check<C: XmtpSharedContext>(context: &C) -> Result<(), ClientError> {
    context.server_configuration().check()?;
    if context.is_closed() {
        return Err(ClientError::AlreadyClosed);
    }
    Ok(())
}

pub(crate) async fn refresh<C: XmtpSharedContext>(
    context: &C,
) -> Result<ServerConfiguration, ClientError> {
    let handle = context.server_configuration();
    // Mock contexts do not bind a request adapter. Production builds always do.
    let fetched = if let Some(fetch) = handle.fetch.get() {
        run(context, fetch.as_ref(), true).await?
    } else {
        run(context, &UnboundFetch(context), true).await?
    };
    Ok(fetched.expect("refresh always fetches configuration"))
}

struct UnboundFetch<'a, C>(&'a C);
#[xmtp_common::async_trait]
impl<C: XmtpSharedContext> ConfigurationFetch for UnboundFetch<'_, C> {
    async fn fetch(&self) -> xmtp_api::Result<xmtp_proto::backend_v1::GetConfigurationResponse> {
        self.0.api().get_configuration().await
    }
    fn backend_url(&self) -> Option<&str> {
        self.0.api().backend_url()
    }
}

// implements: CONF-077
async fn run<C: XmtpSharedContext>(
    context: &C,
    fetch: &dyn ConfigurationFetch,
    refresh: bool,
) -> Result<Option<ServerConfiguration>, ClientError> {
    let handle = context.server_configuration();
    handle.check()?;
    // Ready calls retain the caller's existing lifetime admission. Only a
    // pending check or explicit refresh starts a cancellable foreground call.
    if !refresh && !handle.pending.load(Ordering::Acquire) {
        return Ok(None);
    }
    check(context)?;
    let _foreground = context
        .foreground_calls()
        .enter()
        .ok_or(ClientError::AlreadyClosed)?;
    let cancelled = context.cancellation_token();
    let _admission = tokio::select! {
        biased;
        _ = cancelled.cancelled() => { check(context)?; return Err(ClientError::AlreadyClosed); },
        pending = handle.admission.lock() => pending,
    };
    check(context)?;
    if !handle.pending.load(Ordering::Acquire) && !refresh {
        return Ok(None);
    }
    let response = tokio::select! {
        biased;
        _ = cancelled.cancelled() => { check(context)?; return Err(ClientError::AlreadyClosed); },
        response = fetch.fetch() => response.map_err(|error| ClientError::ConfigurationUnavailable(Box::new(ConfigurationFetchError::Api(error))))?,
    };
    check(context)?;
    // No await from the binding read through the ready transition. The
    // connection exists only for this commit, never for the network wait.
    let fetched = commit_configuration(response, fetch.backend_url(), &context.db(), handle);
    let fetched = match fetched {
        Ok(fetched) => fetched,
        Err(error) => {
            if handle.blocked_connection().is_some() {
                cancelled.cancel();
            }
            return Err(error);
        }
    };
    if let Err(ClientError::ClientVersionTooOld { client, minimum }) =
        check_minimum_version(&fetched, context.version_info().pkg_semver().semver())
    {
        let error =
            handle.block_connection(BlockedConnection::ClientVersionTooOld { client, minimum });
        cancelled.cancel();
        return Err(error);
    }
    handle.pending.store(false, Ordering::Release);
    Ok(Some(fetched))
}

xmtp_common::if_native! {
#[cfg(test)]
mod tests;
}
