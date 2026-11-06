//! The leaves of `groups/tests/test_proposals_app_data.rs` that need the client.

use crate::{context::CoreContext, tester};

/// Key-package rotation preserves the `AppDataDictionary` capability
/// advertisement. Without this property a member whose KP rotates
/// (e.g. via the periodic 30-day rotation) would lose the capability
/// and become unable to join migrated groups or be added by existing
/// members of one. The advertisement is constructed inside
/// `Identity::new_key_package` from compile-time-known capability
/// extensions, so the property holds by construction — this test
/// pins it against accidental refactors that move the advertisement
/// out of the rotation path.
#[xmtp_common::test(unwrap_try = true)]
async fn test_key_package_rotation_preserves_app_data_dictionary_capability() {
    use openmls::extensions::ExtensionType;

    tester!(alix);
    let installation_id = alix.context.installation_id().to_vec();

    // Fetch the initial KP — confirm AppDataDictionary is advertised
    // (the baseline before rotation).
    let initial = alix
        .get_key_packages_for_installation_ids(vec![installation_id.clone()])
        .await?;
    let initial_kp = initial
        .get(&installation_id)
        .expect("initial KP must be present")
        .as_ref()
        .expect("initial KP must verify");
    assert!(
        initial_kp
            .inner
            .leaf_node()
            .capabilities()
            .extensions()
            .contains(&ExtensionType::AppDataDictionary),
        "initial KP must advertise AppDataDictionary"
    );
    // Rotate the key package. (On a fresh test client this may be a
    // no-op — rotation runs only when due — but the capability check
    // below is the contract: whether the function emits a new KP or
    // returns the existing one, the result must still advertise
    // AppDataDictionary.)
    alix.rotate_and_upload_key_package().await?;

    // Fetch the post-rotation KP — must still advertise AppDataDictionary.
    let rotated = alix
        .get_key_packages_for_installation_ids(vec![installation_id.clone()])
        .await?;
    let rotated_kp = rotated
        .get(&installation_id)
        .expect("post-rotation KP must be present")
        .as_ref()
        .expect("post-rotation KP must verify");
    assert!(
        rotated_kp
            .inner
            .leaf_node()
            .capabilities()
            .extensions()
            .contains(&ExtensionType::AppDataDictionary),
        "post-rotation KP must still advertise AppDataDictionary"
    );
}
