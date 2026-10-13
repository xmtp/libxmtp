//! Finding free handles.

use super::{random_handle, scripted_client};
use crate::{
    client::ClientError,
    inbox_handles::{HANDLE_SEARCH_CODES, HandleError},
    tester,
};
use std::collections::HashSet;
use xmtp_id::associations::handle::Handle;

#[xmtp_common::test(unwrap_try = true)]
async fn available_handles_one_lookup_bare_name_first() {
    tester!(alix, disable_workers);
    tester!(bo);
    let (client, script) = scripted_client(&alix).await?;
    let name = random_handle();

    // A user types capitals; they become lowercase.
    let free = client
        .available_handles(&name.as_str().to_ascii_uppercase())
        .await?;
    assert_eq!(script.lock().lookups, vec![HANDLE_SEARCH_CODES + 1]);
    assert_eq!(free.len(), HANDLE_SEARCH_CODES + 1, "a random name is free");
    assert_eq!(free[0], name);
    assert_eq!(free.iter().collect::<HashSet<_>>().len(), free.len());
    assert!(
        free[1..]
            .iter()
            .all(|handle| handle.name() == name.as_str() && handle.as_str() != name.as_str())
    );

    // A held handle is not offered.
    bo.claim_handle(&name).await?;
    let free = client.available_handles(name.as_str()).await?;
    assert!(!free.contains(&name));
    assert_eq!(free.len(), HANDLE_SEARCH_CODES);
}

#[xmtp_common::test(unwrap_try = true)]
async fn available_handles_with_code_checks_only_that_code() {
    tester!(alix, disable_workers);
    let (client, script) = scripted_client(&alix).await?;
    let handle = Handle::with_code(random_handle().name(), 42)?;

    assert_eq!(
        client.available_handles(handle.as_str()).await?,
        vec![handle]
    );
    assert_eq!(script.lock().lookups, vec![1]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn available_handles_rejects_bad_grammar_before_any_request() {
    tester!(alix, disable_workers);
    let (client, script) = scripted_client(&alix).await?;

    for input in ["", "1nick", "nick.eth", "nick#123", "nïck", "nick\n"] {
        assert!(
            matches!(
                client.available_handles(input).await,
                Err(ClientError::Handle(HandleError::Invalid(_)))
            ),
            "{input:?}"
        );
    }
    assert!(script.lock().lookups.is_empty());
}
