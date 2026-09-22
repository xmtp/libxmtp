use super::*;
use sqlx::Acquire;

#[xmtp_common::test(unwrap_try = true)]
async fn maintenance_demand_requires_an_eligible_envelope_above_the_boundary() {
    let fixture = Fixture::new().await?;
    assert!(!dispatcher::has_unsettled_envelopes(&fixture.store, 0).await?);

    let ids = fixture.seed(3, &[1], false).await?;
    sqlx::query("UPDATE envelopes SET push_eligible = false")
        .execute(&fixture.store.primary)
        .await?;
    assert!(!dispatcher::has_unsettled_envelopes(&fixture.store, 0).await?);

    sqlx::query("UPDATE envelopes SET push_eligible = true WHERE sequence_id = $1")
        .bind(ids[1])
        .execute(&fixture.store.primary)
        .await?;
    assert!(dispatcher::has_unsettled_envelopes(&fixture.store, ids[0]).await?);
    assert!(!dispatcher::has_unsettled_envelopes(&fixture.store, ids[1]).await?);
    assert!(!dispatcher::has_unsettled_envelopes(&fixture.store, ids[2]).await?);
}

#[xmtp_common::test(unwrap_try = true)]
async fn idle_poll_uses_bounded_buffers_with_a_generic_plan() {
    const ENVELOPE_COUNT: i64 = 20_000;
    // A full scan of this fixture reads hundreds of pages. Leave room for
    // index height and visibility checks without depending on exact plan text.
    const MAX_BUFFER_ACCESSES: u64 = 32;
    let fixture = Fixture::new().await?;
    let ids = fixture.seed(ENVELOPE_COUNT, &[1], false).await?;
    sqlx::query("VACUUM ANALYZE envelopes")
        .execute(&fixture.store.primary)
        .await?;

    let mut connection = fixture.store.read.acquire().await?;
    let mut transaction = connection.begin().await?;
    sqlx::query("SET LOCAL plan_cache_mode = force_generic_plan")
        .execute(&mut *transaction)
        .await?;
    // Only the checked-in statement and an integer enter these SQL commands.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "PREPARE idle_poll(bigint) AS {}",
        include_str!("../unsettled.sql")
    )))
    .execute(&mut *transaction)
    .await?;
    let boundary = ids.iter().max()?;
    let plan: serde_json::Value = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "EXPLAIN (ANALYZE, BUFFERS, TIMING OFF, FORMAT JSON) EXECUTE idle_poll({boundary})"
    )))
    .fetch_one(&mut *transaction)
    .await?;
    let root = &plan[0]["Plan"];
    let accesses = root["Shared Hit Blocks"].as_u64()? + root["Shared Read Blocks"].as_u64()?;
    assert!(
        accesses <= MAX_BUFFER_ACCESSES,
        "idle poll used {accesses} buffers for {ENVELOPE_COUNT} envelopes: {plan}"
    );
    let generic_plans: i64 = sqlx::query_scalar(
        "SELECT generic_plans FROM pg_prepared_statements WHERE name = 'idle_poll'",
    )
    .fetch_one(&mut *transaction)
    .await?;
    assert_eq!(generic_plans, 1);
    sqlx::query("DEALLOCATE idle_poll")
        .execute(&mut *transaction)
        .await?;
    transaction.rollback().await?;
}
