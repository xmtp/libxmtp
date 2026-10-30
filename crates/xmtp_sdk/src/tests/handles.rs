//! Handles through the SDK surface. The behaviour lives in core
//! (`xmtp_mls::inbox_handles`); these tests check that every SDK call reaches
//! it, keeps the public shape, and maps its errors to public codes.
//!
//! A claim on the shared backend is permanent, so tests there use random
//! handles and claim at most once per inbox. Tests that claim more than once
//! per inbox use a disposable backend and move its identity history back in
//! server time past the interval between claims.

mod errors;
mod lying_backend;
mod statics;

use super::*;
use std::collections::{HashMap, HashSet};

/// Longer than the backend's interval of 7 days between two claims by one
/// inbox. Aging an inbox's identity history by it admits the next claim.
const PAST_CLAIM_INTERVAL: Duration = Duration::from_secs(8 * 24 * 60 * 60);

/// A handle that no other test run claims.
fn random_handle() -> String {
    format!("t{}", hex::encode(xmtp_common::rand_vec::<10>()))
}

fn handle_identity(text: &str) -> PublicIdentity {
    PublicIdentity {
        identifier: text.into(),
        kind: PublicIdentityKind::Handle,
    }
}

/// A backend with its own database, for tests that claim more than once per
/// inbox or need a non-default handle setting.
struct DisposableBackend(xmtp_backend::test_support::TestServer);

impl DisposableBackend {
    async fn start(toml: &str) -> Result<Self, XmtpError> {
        xmtp_backend::test_support::TestServer::from_toml(toml)
            .await
            .map(Self)
            .map_err(XmtpError::unknown)
    }

    fn source(&self) -> BackendSource {
        source_at(&self.0.url)
    }

    fn options(&self) -> ClientOptions {
        options_at(&self.0.url)
    }

    /// A second backend on this database with `change` applied to the
    /// configuration, as an operator restart with a new setting gives.
    async fn restarted(
        &self,
        change: impl FnOnce(&mut xmtp_backend::config::Config),
    ) -> Result<xmtp_backend::test_support::RunningServer, XmtpError> {
        self.0
            .restart_with(change)
            .await
            .map_err(XmtpError::unknown)
    }

    /// Move `inbox`'s identity history back past the interval between
    /// claims, so its next claim is admitted.
    async fn age(&self, inbox: &InboxId) -> Result<(), XmtpError> {
        self.0
            .age_identity_history(inbox.checked()?, PAST_CLAIM_INTERVAL)
            .await
            .map_err(XmtpError::unknown)
    }

    async fn stop(self) -> Result<(), XmtpError> {
        self.0.stop().await.map_err(XmtpError::unknown)
    }
}

fn source_at(url: &str) -> BackendSource {
    BackendSource::Options {
        options: BackendOptions {
            url: url.to_owned(),
            app_version: None,
            credentials: None,
            credential: None,
        },
    }
}

fn options_at(url: &str) -> ClientOptions {
    ClientOptions {
        backend: Some(source_at(url)),
        ..options()
    }
}

fn shared_source() -> BackendSource {
    options().backend.expect("test options name a backend")
}

/// Claim `handle` for `client`'s inbox, then wait until the backend's
/// lookups see the claim. A lookup can answer from a read replica that lags
/// the claim's write for a moment. The wait asks core directly, so it does
/// not depend on the SDK code under test.
async fn claim_visible(client: &Client, handle: &str) -> Result<(), XmtpError> {
    client.claim_handle(handle.to_owned()).await?;
    let handle = xmtp_id::associations::handle::Handle::parse(handle)
        .map_err(|_| XmtpError::invalid_argument("invalid handle"))?;
    let lookups = async {
        while !client
            .inner
            .handles_have_owner(std::slice::from_ref(&handle))
            .await
            .map_err(XmtpError::from_client)?[0]
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok::<_, XmtpError>(())
    };
    tokio::time::timeout(Duration::from_secs(10), lookups)
        .await
        .map_err(|_| XmtpError::unknown("the claim did not reach the lookups"))?
}

/// Requests that would show an account call reached the backend.
async fn request_counts(client: &Client) -> Result<(u64, u64, u64), XmtpError> {
    let diagnostics = client.diagnostics();
    let api = diagnostics.api_statistics().await?;
    let identity = diagnostics.identity_statistics().await?;
    Ok((api.publish, api.query, identity.get_inbox_ids))
}

async fn epoch(group: &crate::Group) -> Result<u64, XmtpError> {
    Ok(group.debug_info().await?.epoch)
}

async fn member_ids(group: &crate::Group) -> Result<HashSet<InboxId>, XmtpError> {
    Ok(group
        .members()
        .await?
        .into_iter()
        .map(|member| member.inbox_id)
        .collect())
}

/// A signer whose identity is a handle. A handle signs nothing, so it never
/// signs.
struct HandleSigner(PublicIdentity);

#[xmtp_common::async_trait]
impl Signer for HandleSigner {
    async fn identity(&self) -> Result<PublicIdentity, SignerError> {
        Ok(self.0.clone())
    }

    async fn kind(&self) -> Result<SignerKind, SignerError> {
        Ok(SignerKind::Eoa)
    }

    async fn sign(&self, _request: SigningRequest) -> Result<Signature, SignerError> {
        Err(SignerError::Failed)
    }
}

#[track_caller]
fn assert_invalid_argument<T>(result: Result<T, XmtpError>, call: &str) {
    match result {
        Err(XmtpError::InvalidArgument(_)) => {}
        Err(other) => panic!("{call} must fail with InvalidArgument, got {other:?}"),
        Ok(_) => panic!("{call} must fail with InvalidArgument, but it succeeded"),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn account_calls_reject_handles() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    // A grammatical handle, and the text of a registered account given the
    // handle kind: neither may act as an account.
    let handles = [
        handle_identity(&random_handle()),
        handle_identity(&bo.identity().identifier),
    ];
    // A backend that answers nothing: a call that sent a request first would
    // fail with a network error, not InvalidArgument.
    let unreachable = BackendOptions {
        url: "http://127.0.0.1:9".into(),
        app_version: None,
        credentials: None,
        credential: None,
    };
    let unreachable_options = ClientOptions {
        backend: Some(BackendSource::Options {
            options: unreachable.clone(),
        }),
        ..options()
    };
    let recovery = crate::generate_local_signer().await;
    let before = request_counts(&alix).await?;
    for handle in handles {
        let signer: Arc<dyn Signer> = Arc::new(HandleSigner(handle.clone()));
        assert_invalid_argument(signer::identity(signer.clone()).await, "Signer::identity");
        assert_invalid_argument(
            Client::create(signer.clone(), unreachable_options.clone()).await,
            "Client::create",
        );
        assert_invalid_argument(
            Client::build(handle.clone(), unreachable_options.clone(), None).await,
            "Client::build",
        );
        assert_invalid_argument(
            crate::generate_inbox_id(handle.clone(), None),
            "generate_inbox_id",
        );
        assert_invalid_argument(
            crate::static_helpers::revoke_installations_with_backend(
                BackendSource::Options {
                    options: unreachable.clone(),
                },
                signer.clone(),
                alix.inbox_id(),
                vec![],
            )
            .await,
            "revoke_installations_with_backend",
        );
        assert_invalid_argument(
            alix.unsafe_add_account(signer.clone(), true).await,
            "unsafe_add_account",
        );
        assert_invalid_argument(
            alix.unsafe_add_account_signature_request(handle.clone(), true)
                .await
                .map(|_| ()),
            "unsafe_add_account_signature_request",
        );
        assert_invalid_argument(
            alix.remove_account(recovery.clone(), handle.clone()).await,
            "remove_account",
        );
        assert_invalid_argument(
            alix.unsafe_remove_account_signature_request(handle.clone())
                .await
                .map(|_| ()),
            "unsafe_remove_account_signature_request",
        );
        assert_invalid_argument(
            alix.change_recovery_identifier(recovery.clone(), handle.clone())
                .await,
            "change_recovery_identifier",
        );
        assert_invalid_argument(
            alix.unsafe_change_recovery_identifier_signature_request(handle.clone())
                .await
                .map(|_| ()),
            "unsafe_change_recovery_identifier_signature_request",
        );
    }
    assert_eq!(
        request_counts(&alix).await?,
        before,
        "an account call with a handle sent a request"
    );
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn handle_routes_reach_the_claimed_inbox() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let handle = random_handle();
    claim_visible(&bo, &handle).await?;
    let by_handle = handle_identity(&handle);
    let conversations = alix.conversations();

    assert_eq!(
        alix.inbox_id_for(by_handle.clone()).await?,
        Some(bo.inbox_id())
    );
    let dm = conversations
        .create_dm_with_identity(by_handle.clone(), None)
        .await?;
    assert_eq!(dm.peer_inbox_id().await?, Some(bo.inbox_id()));
    let by_inbox = conversations.create_dm(bo.inbox_id(), None).await?;
    assert_eq!(by_inbox.id(), dm.id(), "the DM is the one by inbox ID");
    let found = conversations.get_dm_by_identity(by_handle.clone()).await?;
    assert_eq!(found.map(|dm| dm.id()), Some(dm.id()));

    let created = conversations
        .create_group_with_identities(vec![by_handle.clone()], None)
        .await?;
    assert_eq!(
        member_ids(&created).await?,
        HashSet::from([alix.inbox_id(), bo.inbox_id()])
    );

    let group = conversations.create_group(vec![], None).await?;
    let start = epoch(&group).await?;
    let added = group
        .add_members_by_identity(vec![by_handle.clone()])
        .await?;
    assert_eq!(added.added, vec![bo.inbox_id()]);
    assert_eq!(epoch(&group).await?, start + 1, "an add is one commit");
    assert!(member_ids(&group).await?.contains(&bo.inbox_id()));
    group.remove_members_by_identity(vec![by_handle]).await?;
    assert_eq!(epoch(&group).await?, start + 2, "a remove is one commit");
    assert_eq!(member_ids(&group).await?, HashSet::from([alix.inbox_id()]));
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn mixed_list_is_one_commit() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let caro = Client::create(crate::generate_local_signer().await, options()).await?;
    let handle = random_handle();
    claim_visible(&bo, &handle).await?;
    let mixed = vec![handle_identity(&handle), caro.identity()];
    let everyone = HashSet::from([alix.inbox_id(), bo.inbox_id(), caro.inbox_id()]);
    let conversations = alix.conversations();

    let group = conversations.create_group(vec![], None).await?;
    let start = epoch(&group).await?;
    let added = group.add_members_by_identity(mixed.clone()).await?;
    assert_eq!(epoch(&group).await?, start + 1, "a mixed add is one commit");
    assert_eq!(
        added.added.into_iter().collect::<HashSet<_>>(),
        HashSet::from([bo.inbox_id(), caro.inbox_id()])
    );
    assert_eq!(member_ids(&group).await?, everyone);

    // A create from accounts only is the reference for one membership commit.
    let accounts = conversations
        .create_group_with_identities(vec![bo.identity(), caro.identity()], None)
        .await?;
    let created = conversations
        .create_group_with_identities(mixed, None)
        .await?;
    assert_eq!(member_ids(&created).await?, everyone);
    assert_eq!(
        epoch(&created).await?,
        epoch(&accounts).await?,
        "a mixed create adds its members in one commit"
    );
    alix.end().await?;
    bo.end().await?;
    caro.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn can_message_keys_handles_by_canonical_text() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let claimed = random_handle();
    claim_visible(&bo, &claimed).await?;
    let unclaimed = random_handle();
    // The capitals are input; the key holds the canonical handle.
    let identities = vec![
        handle_identity(&claimed.to_uppercase()),
        handle_identity(&unclaimed),
        bo.identity(),
    ];
    let verify = |result: HashMap<String, bool>| {
        assert_eq!(result.len(), 3, "{result:?}");
        assert!(result[&format!("handle:{claimed}")]);
        assert!(!result[&format!("handle:{unclaimed}")]);
        assert!(result[&format!("ethereum:{}", bo.identity().identifier)]);
    };
    verify(alix.can_message(identities.clone()).await?);
    verify(crate::static_helpers::can_message_with_backend(shared_source(), identities).await?);

    let before = request_counts(&alix).await?;
    let malformed = vec![bo.identity(), handle_identity("not a handle")];
    assert_invalid_argument(alix.can_message(malformed.clone()).await, "can_message");
    assert_invalid_argument(
        alix.inbox_id_for(malformed[1].clone()).await,
        "inbox_id_for",
    );
    assert_eq!(request_counts(&alix).await?, before);
    assert_invalid_argument(
        crate::static_helpers::can_message_with_backend(shared_source(), malformed).await,
        "can_message_with_backend",
    );
    alix.end().await?;
    bo.end().await?;
}

// A claim reads its input as every other handle input does: the capitals
// a user types are lowercased, and the log holds the canonical handle.
#[xmtp_common::test(unwrap_try = true)]
async fn claim_stores_the_lowercase_form_of_a_mixed_case_handle() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let tail = random_handle();
    let typed = format!("Nick{}", tail.to_uppercase());
    let canonical = format!("nick{tail}");
    alix.claim_handle(typed).await?;

    let own = alix.inbox_state(true).await?;
    assert_eq!(own.handle.as_deref(), Some(canonical.as_str()));
    assert!(own.other_handles.is_empty(), "{:?}", own.other_handles);
    // Another client derives the same handle from the stored log.
    let peer = bo.inbox_states(vec![alix.inbox_id()], true).await?;
    assert_eq!(peer[0].handle.as_deref(), Some(canonical.as_str()));
    alix.end().await?;
    bo.end().await?;
}

// The static lookup covers the verified resolver for a caller with no
// client (`resolve_verified_with_verifier`).
#[xmtp_common::test(unwrap_try = true)]
async fn inbox_id_for_old_and_unclaimed_handles() {
    let backend = DisposableBackend::start("").await?;
    let alix = Client::create(crate::generate_local_signer().await, backend.options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, backend.options()).await?;
    let (old, current) = (random_handle(), random_handle());
    bo.claim_handle(old.clone()).await?;
    backend.age(&bo.inbox_id()).await?;
    bo.claim_handle(current.clone()).await?;

    for handle in [&old, &current] {
        assert_eq!(
            alix.inbox_id_for(handle_identity(handle)).await?,
            Some(bo.inbox_id()),
            "the instance lookup resolves {handle}"
        );
        assert_eq!(
            crate::static_helpers::inbox_id_for_with_backend(
                backend.source(),
                handle_identity(handle)
            )
            .await?,
            bo.inbox_id(),
            "the static lookup resolves {handle}"
        );
    }
    let unclaimed = handle_identity(&random_handle());
    assert_eq!(alix.inbox_id_for(unclaimed.clone()).await?, None);
    let error = crate::static_helpers::inbox_id_for_with_backend(backend.source(), unclaimed)
        .await
        .expect_err("an unclaimed handle has no inbox");
    assert!(
        matches!(&error, XmtpError::IdentityNotFound(details) if details.code == "IdentityNotFound" && !details.retryable),
        "got {error:?}"
    );
    alix.end().await?;
    bo.end().await?;
    backend.stop().await?;
}

// verifies: IDENT-086
#[xmtp_common::test(unwrap_try = true)]
async fn inbox_state_and_members_split_current_and_other_handles() {
    let backend = DisposableBackend::start("").await?;
    let alix = Client::create(crate::generate_local_signer().await, backend.options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, backend.options()).await?;
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    let (first, second, third) = (random_handle(), random_handle(), random_handle());
    bo.claim_handle(first.clone()).await?;
    backend.age(&bo.inbox_id()).await?;
    bo.claim_handle(second.clone()).await?;
    backend.age(&bo.inbox_id()).await?;
    bo.claim_handle(third.clone()).await?;

    let current = Some(third);
    let others = vec![second, first];
    let own = bo.inbox_state(false).await?;
    assert_eq!((&own.handle, &own.other_handles), (&current, &others));
    let peer = alix.inbox_states(vec![bo.inbox_id()], true).await?;
    assert_eq!(
        (&peer[0].handle, &peer[0].other_handles),
        (&current, &others)
    );
    let fetched =
        crate::static_helpers::inbox_states_with_backend(backend.source(), vec![bo.inbox_id()])
            .await?;
    assert_eq!(
        (&fetched[0].handle, &fetched[0].other_handles),
        (&current, &others)
    );

    // Alix now holds Bo's newest state, which is newer than the group's.
    let members = group.members().await?;
    let bo_member = members
        .iter()
        .find(|member| member.inbox_id == bo.inbox_id())
        .expect("bo is a member");
    assert_eq!(
        (&bo_member.handle, &bo_member.other_handles),
        (&current, &others)
    );
    let alix_member = members
        .iter()
        .find(|member| member.inbox_id == alix.inbox_id())
        .expect("alix is a member");
    assert_eq!(alix_member.handle, None);
    assert!(alix_member.other_handles.is_empty());
    let alix_state = alix.inbox_state(false).await?;
    assert_eq!(alix_state.handle, None);
    assert!(alix_state.other_handles.is_empty());
    alix.end().await?;
    bo.end().await?;
    backend.stop().await?;
}

/// The next `handle.changed` event, or `None` when none arrives in time.
async fn next_handle_change(
    reader: &crate::EventReader,
    wait: Duration,
) -> Result<Option<crate::HandleChanged>, XmtpError> {
    match tokio::time::timeout(wait, reader.next()).await {
        Err(_) => Ok(None),
        Ok(event) => match event? {
            Some(ClientEvent::HandleChanged { handle_changed }) => Ok(Some(handle_changed)),
            other => panic!("expected handle.changed, got {other:?}"),
        },
    }
}

// verifies: EVENT-058
#[xmtp_common::test(unwrap_try = true)]
async fn handle_changed_reaches_a_subscription() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    // Bo holds a state of Alix's inbox before the claim.
    let before = bo.inbox_states(vec![alix.inbox_id()], true).await?;
    assert_eq!(before[0].handle, None);
    let own = alix
        .events(event_filter(vec![EventKind::HandleChanged]))
        .await?;
    let peer = bo
        .events(event_filter(vec![EventKind::HandleChanged]))
        .await?;

    let handle = random_handle();
    alix.claim_handle(handle.clone()).await?;
    bo.inbox_states(vec![alix.inbox_id()], true).await?;

    for reader in [&own, &peer] {
        let change = next_handle_change(reader, Duration::from_secs(10))
            .await?
            .expect("a handle.changed event");
        assert_eq!(change.inbox_id, alix.inbox_id());
        assert_eq!(change.handle, handle);
        assert!(
            next_handle_change(reader, Duration::from_millis(300))
                .await?
                .is_none(),
            "one claim is one handle.changed event"
        );
    }
    own.end().await?;
    peer.end().await?;
    alix.end().await?;
    bo.end().await?;
}

// verifies: CONF-081
#[xmtp_common::test(unwrap_try = true)]
async fn server_configuration_exposes_min_name_length() {
    let backend = DisposableBackend::start(
        r#"
        [handles]
        min_name_length = 5
    "#,
    )
    .await?;
    let alix = Client::create(crate::generate_local_signer().await, backend.options()).await?;
    assert_eq!(alix.server_configuration().handles.min_name_length, 5);
    assert_eq!(
        crate::client_identity::fetch_server_configuration(backend.source())
            .await?
            .handles
            .min_name_length,
        5
    );

    // A name shorter than the minimum is neither hidden nor refused by the
    // SDK. Only the backend refuses the claim.
    let short = "ab".to_owned();
    let available = alix.available_handles(short.clone()).await?;
    assert_eq!(available.first(), Some(&short));
    let (publishes, _, _) = request_counts(&alix).await?;
    let refused = alix.claim_handle(short).await;
    assert!(
        matches!(refused, Err(XmtpError::HandleNotAllowed(_))),
        "got {refused:?}"
    );
    let (published, _, _) = request_counts(&alix).await?;
    assert!(published > publishes, "the claim reached the backend");
    alix.end().await?;
    backend.stop().await?;
}

// A raised minimum leaves a shorter handle claimed before it valid. A client
// whose snapshot holds the raised minimum still resolves that handle, adds a
// member by it, and returns it in every result.
// verifies: CONF-081
#[xmtp_common::test(unwrap_try = true)]
async fn handle_shorter_than_a_raised_minimum_stays_in_results() {
    let backend = DisposableBackend::start(
        r#"
        [handles]
        min_name_length = 1
    "#,
    )
    .await?;
    let alix = Client::create(crate::generate_local_signer().await, backend.options()).await?;
    let short = "ab".to_owned();
    alix.claim_handle(short.clone()).await?;
    let mut raised = backend
        .restarted(|config| config.handles.min_name_length = 5)
        .await?;
    let bo = Client::create(
        crate::generate_local_signer().await,
        options_at(&raised.url),
    )
    .await?;
    assert_eq!(bo.server_configuration().handles.min_name_length, 5);

    let source = source_at(&raised.url);
    let target = handle_identity(&short);
    assert_eq!(
        bo.inbox_id_for(target.clone()).await?,
        Some(alix.inbox_id())
    );
    assert_eq!(
        crate::static_helpers::inbox_id_for_with_backend(source.clone(), target.clone()).await?,
        alix.inbox_id()
    );
    let reachable = HashMap::from([(format!("handle:{short}"), true)]);
    assert_eq!(bo.can_message(vec![target.clone()]).await?, reachable);
    assert_eq!(
        crate::static_helpers::can_message_with_backend(source.clone(), vec![target.clone()])
            .await?,
        reachable
    );
    let peer = bo.inbox_states(vec![alix.inbox_id()], true).await?;
    assert_eq!(peer[0].handle.as_ref(), Some(&short));
    let fetched =
        crate::static_helpers::inbox_states_with_backend(source, vec![alix.inbox_id()]).await?;
    assert_eq!(fetched[0].handle.as_ref(), Some(&short));

    let group = bo
        .conversations()
        .create_group_with_identities(vec![target], None)
        .await?;
    let members = group.members().await?;
    let alix_member = members
        .iter()
        .find(|member| member.inbox_id == alix.inbox_id())
        .expect("the short handle added alix");
    assert_eq!(alix_member.handle.as_ref(), Some(&short));
    alix.end().await?;
    bo.end().await?;
    raised.stop().await.map_err(XmtpError::unknown)?;
    backend.stop().await?;
}
