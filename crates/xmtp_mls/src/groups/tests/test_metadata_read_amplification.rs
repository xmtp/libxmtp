//! Measurement for the metadata read-amplification fix (plan "L").
//!
//! Group-metadata properties used to reach their value through a full
//! `OpenMlsGroup::load` (ratchet tree + secrets + extensions) even though the
//! value lives only in the `GroupContext` extensions. The public accessors now
//! read only the `GroupContext` (`load_group_context`). This test measures the
//! KV read round-trips of the real public API against the retained full-load
//! baseline (`mutable_metadata_via_full_load`).
//!
//! The KV read counter (`xmtp_db::sql_key_store::count_kv_reads`) is a tokio
//! task-local compiled in only under `test`/`test-utils`; it does not exist in
//! release builds, so this measurement adds nothing to the production read path.

use crate::groups::GroupError;
use crate::tester;
use xmtp_db::sql_key_store::count_kv_reads_async;

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn metadata_read_amplification() {
    tester!(alix);
    let group = alix.create_group(None, None).await?;
    group
        .update_group_name("Perf Test Group".to_string())
        .await?;
    group
        .update_group_description("measuring reads".to_string())
        .await?;

    // Correctness: the context-read path returns the same metadata as the old
    // full-load path, and the public accessors still return the set values.
    let baseline = group.mutable_metadata_via_full_load().await?;
    let now = group.mutable_metadata().await?;
    assert_eq!(baseline.attributes, now.attributes, "attributes must match");
    assert_eq!(baseline.admin_list, now.admin_list, "admin_list must match");
    assert_eq!(
        baseline.super_admin_list, now.super_admin_list,
        "super_admin_list must match"
    );
    assert_eq!(group.group_name().await?, "Perf Test Group");
    assert_eq!(group.group_description().await?, "measuring reads");
    // permissions() now also reads the context; just ensure it still works.
    let _ = group.permissions().await?;

    // Baseline: one metadata fetch via the old full `OpenMlsGroup::load`.
    let (res, one_full_load) =
        count_kv_reads_async(group.mutable_metadata_via_full_load()).await;
    res?;

    // New snapshot: one context read yields every mutable-metadata field.
    let (res, snapshot) = count_kv_reads_async(group.mutable_metadata()).await;
    res?;

    // A realistic conversation-header render, through the REAL public API.
    let me = alix.inbox_id().to_string();
    let (res, header_public) = count_kv_reads_async(async {
        group.group_name().await?;
        group.group_description().await?;
        group.admin_list().await?;
        group.super_admin_list().await?;
        group.is_admin(me).await?;
        group.permissions().await?;
        Ok::<(), GroupError>(())
    })
    .await;
    res?;

    let old_header = one_full_load * 6; // each of the 6 accessors used to full-load
    tracing::info!(
        one_full_load,
        snapshot,
        header_public,
        old_header,
        "read amplification: single fetch full-load={one_full_load} -> snapshot={snapshot}; \
         6-call public header {old_header} -> {header_public} reads"
    );

    assert_eq!(snapshot, 1, "context snapshot must be exactly 1 KV read");
    assert!(
        header_public < one_full_load,
        "6-accessor public header ({header_public}) should cost less than ONE old full load ({one_full_load})"
    );
}
