# API test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

| File | Qualified test | Form / gates / cases | Requirements |
| --- | --- | --- | --- |
| `crates/xmtp_api_backend/src/middleware/readonly_client.rs` | `middleware::readonly_client::tests::test_forwards_to_inner` | rstest fixture + custom async; `unwrap_try | `API-REQ-021` |
| `crates/xmtp_api_backend/src/middleware/readonly_client.rs` | `middleware::readonly_client::tests::test_errors_on_write` | rstest fixture + custom async; `unwrap_try | `API-REQ-021` |
| `crates/xmtp_api_backend/src/middleware/auth.rs` | `middleware::auth::tests::test_auth_handle` | custom async | `API-REQ-025` |
| `crates/xmtp_api_backend/src/middleware/auth.rs` | `middleware::auth::tests::test_auth_middleware_no_callback_or_handle` | custom async; native-only macro block | `API-REQ-025` |
| `crates/xmtp_api_backend/src/middleware/auth.rs` | `middleware::auth::tests::test_auth_middleware_with_no_callback_and_handle` | custom async | `API-REQ-025` |
| `crates/xmtp_api_backend/src/middleware/auth.rs` | `middleware::auth::tests::test_auth_middleware_with_callback_and_no_handle` | custom async | `API-REQ-025` |
| `crates/xmtp_api_backend/src/middleware/auth.rs` | `middleware::auth::tests::test_auth_middleware_with_callback_and_handle` | custom async | `API-REQ-025` |
| `crates/xmtp_api_backend/src/middleware/auth.rs` | `middleware::auth::tests::test_auth_middleware_with_callback_and_handle_concurrent_requests` | custom async | `API-REQ-025` |
| `crates/xmtp_api_backend/src/queries/bidi.rs` | `queries::bidi::tests::probe_nonces_never_mint_the_watchdog_nonce` | Backend protocol; native test | `API-REQ-056` |
| `crates/xmtp_api_backend/src/queries/bidi.rs` | `queries::bidi::tests::drain_after_finish_flushes_pending_before_closing` | Backend protocol; native test | `API-REQ-056` |
| `crates/xmtp_api_backend/src/queries/bidi.rs` | `queries::bidi::tests::drain_after_finish_bounds_the_flush_on_a_wedged_wire` | Backend protocol; native test | `API-REQ-056` |
| `crates/xmtp_api_backend/src/queries/bidi.rs` | `queries::bidi::tests::watchdog_probes_then_tears_down_a_silent_wire` | Backend protocol; native test | `API-REQ-057` |
| `crates/xmtp_api_backend/src/queries/bidi.rs` | `queries::bidi::tests::an_answered_watchdog_probe_keeps_the_wire_alive` | Backend protocol; native test | `API-REQ-057` |
| `crates/xmtp_api_backend/src/queries/bidi.rs` | `queries::bidi::tests::inbound_activity_resets_the_watchdog` | Backend protocol; native test | `API-REQ-057` |
| `crates/xmtp_api_backend/src/queries/bidi.rs` | `queries::bidi::tests::consumer_backpressure_is_not_wire_silence` | Backend protocol; native test | `API-REQ-057` |
| `crates/xmtp_api_backend/src/queries/backend/transport.rs` | `queries::backend::transport::tests::encodes_outbound_and_decodes_inbound` | Backend protocol; native test | `API-REQ-055`, `API-REQ-010` |
| `crates/xmtp_api_backend/src/queries/backend/transport.rs` | `queries::backend::transport::tests::tags_open_error_with_subscribe_endpoint` | Backend protocol; native test | `API-REQ-055` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::open_sends_initial_mutate_and_emits_started` | Backend protocol; native test | `API-REQ-058` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::auto_pongs_server_ping_without_surfacing_it` | Backend protocol; native test | `API-REQ-058` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::probe_round_trips_and_pong_is_not_an_event` | Backend protocol; native test | `API-REQ-058` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::mutate_is_forwarded_to_the_wire` | Backend protocol; native test | `API-REQ-058` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::inbound_error_closes_the_connection` | Backend protocol; native test | `API-REQ-059` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::closing_inbound_tears_down_sends` | Backend protocol; native test | `API-REQ-059` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::concurrent_mutate_and_probe_both_reach_the_wire` | Backend protocol; native test | `API-REQ-058`, `API-REQ-059` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::probe_within_times_out_when_no_pong` | Backend protocol; native test | `API-REQ-059` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::default_probe_timeout_tracks_server_keepalive` | Backend protocol; native test | `API-REQ-059` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::busy_wire_does_not_stall_inbound` | Backend protocol; native test | `API-REQ-059` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::gives_up_when_wire_wedged_past_backlog_cap` | Backend protocol; native test | `API-REQ-059` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::finish_is_processed_under_wire_backpressure` | Backend protocol; native test | `API-REQ-059` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::mutate_and_probe_report_closed_after_finish` | Backend protocol; native test | `API-REQ-059` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::finish_resolves_in_flight_probe_to_closed` | Backend protocol; native test | `API-REQ-059` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::try_mutate_reports_full_and_recovers_after_drain` | Backend protocol; native test | `API-REQ-059` |
| `crates/xmtp_api_backend/src/queries/backend/connection.rs` | `queries::backend::connection::tests::try_mutate_reports_closed_after_finish` | Backend protocol; native test | `API-REQ-059` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/delivery.rs` | `queries::bidi_transport::tests::delivery::first_lease_opens_the_wire_with_its_cursored_adds` | Backend protocol; native test | `API-REQ-060` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/limits.rs` | `queries::bidi_transport::tests::limits::a_lease_over_the_cap_splits_into_bounded_frames` | Backend protocol; native test | `API-REQ-061` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/limits.rs` | `queries::bidi_transport::tests::limits::a_reconnect_resume_over_the_cap_splits_into_bounded_frames` | Backend protocol; native test | `API-REQ-061` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/limits.rs` | `queries::bidi_transport::tests::limits::an_over_cap_lease_still_catching_up_survives_a_wire_death` | Backend protocol; native test | `API-REQ-061`, `API-REQ-070` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/limits.rs` | `queries::bidi_transport::tests::limits::an_over_cap_lease_survives_suspend_and_resume` | Backend protocol; native test | `API-REQ-061`, `API-REQ-067` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/limits.rs` | `queries::bidi_transport::tests::limits::a_lease_over_the_byte_budget_splits_into_bounded_frames` | Backend protocol; native test | `API-REQ-061` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/limits.rs` | `queries::bidi_transport::tests::limits::a_mass_unsubscribe_chunks_the_removes_update` | Backend protocol; native test | `API-REQ-061` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/delivery.rs` | `queries::bidi_transport::tests::delivery::second_lease_is_a_cursored_re_add_on_the_open_wire` | Backend protocol; native test | `API-REQ-062` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/delivery.rs` | `queries::bidi_transport::tests::delivery::deliveries_demux_by_topic` | Backend protocol; native test | `API-REQ-063` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/delivery.rs` | `queries::bidi_transport::tests::delivery::rotation_ordered_replay_delivers_every_topic` | Backend protocol; native test | `API-REQ-064` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/delivery.rs` | `queries::bidi_transport::tests::delivery::covered_live_frame_is_dropped` | Backend protocol; native test | `API-REQ-064` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/delivery.rs` | `queries::bidi_transport::tests::delivery::shared_topic_fans_out_to_every_lease` | Backend protocol; native test | `API-REQ-063` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/delivery.rs` | `queries::bidi_transport::tests::delivery::markers_route_to_their_owners` | Backend protocol; native test | `API-REQ-063` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/delivery.rs` | `queries::bidi_transport::tests::delivery::deref_is_refcounted_and_last_lease_closes_the_wire` | Backend protocol; native test | `API-REQ-065` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/backpressure.rs` | `queries::bidi_transport::tests::backpressure::slow_lease_is_dropped_without_blocking_siblings` | Backend protocol; native test | `API-REQ-065`, `API-REQ-077` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/reconnect.rs` | `queries::bidi_transport::tests::reconnect::wire_death_reopens_from_lease_floors` | Backend protocol; native test | `API-REQ-066` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/reconnect.rs` | `queries::bidi_transport::tests::reconnect::command_traffic_does_not_postpone_the_reconnect` | Backend protocol; native test | `API-REQ-066` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/reconnect.rs` | `queries::bidi_transport::tests::reconnect::half_open_wire_is_reaped_and_reconnected` | Backend protocol; native test | `API-REQ-066` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/suspend.rs` | `queries::bidi_transport::tests::suspend::suspend_half_closes_and_resume_completes_at_catch_up` | Backend protocol; native test | `API-REQ-067` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/delivery.rs` | `queries::bidi_transport::tests::delivery::wire_session_span_closes_with_a_reason_on_every_release_path` | Backend protocol; native test | `API-REQ-066` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/suspend.rs` | `queries::bidi_transport::tests::suspend::concurrent_resumes_join_one_catch_up_update` | Backend protocol; native test | `API-REQ-068` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/suspend.rs` | `queries::bidi_transport::tests::suspend::suspended_transport_stays_off_the_network` | Backend protocol; native test | `API-REQ-067` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/suspend.rs` | `queries::bidi_transport::tests::suspend::a_born_suspended_transport_parks_the_first_lease` | Backend protocol; native test | `API-REQ-068` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/suspend.rs` | `queries::bidi_transport::tests::suspend::dropping_the_last_lease_settles_resume_waiters` | Backend protocol; native test | `API-REQ-068` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/suspend.rs` | `queries::bidi_transport::tests::suspend::suspend_preempts_a_stuck_dial` | Backend protocol; native test | `API-REQ-068` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/suspend.rs` | `queries::bidi_transport::tests::suspend::a_preempting_suspend_outranks_a_deferred_resume` | Backend protocol; native test | `API-REQ-068` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/backoff.rs` | `queries::bidi_transport::tests::backoff::a_resume_burst_during_an_outage_dials_once` | Backend protocol; native test | `API-REQ-069` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/suspend.rs` | `queries::bidi_transport::tests::suspend::resume_with_nothing_to_do_resolves_immediately` | Backend protocol; native test | `API-REQ-069` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/reconnect.rs` | `queries::bidi_transport::tests::reconnect::reconnect_folds_a_caught_up_holders_floor_when_nothing_was_delivered` | Backend protocol; native test | `API-REQ-072` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/suspend.rs` | `queries::bidi_transport::tests::suspend::lease_during_a_dead_wire_rides_the_resume_open` | Backend protocol; native test | `API-REQ-074` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/reconnect.rs` | `queries::bidi_transport::tests::reconnect::deref_during_a_dead_wire_keeps_the_topic_off_the_reconnect` | Backend protocol; native test | `API-REQ-074` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/delivery.rs` | `queries::bidi_transport::tests::delivery::deref_purges_only_the_dropped_leases_unsent_updates` | Backend protocol; native test | `API-REQ-074` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/delivery.rs` | `queries::bidi_transport::tests::delivery::empty_lease_is_refused_without_opening_the_wire` | Backend protocol; native test | `API-REQ-075` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/reconnect.rs` | `queries::bidi_transport::tests::reconnect::open_failure_surfaces_and_registers_nothing` | Backend protocol; native test | `API-REQ-075` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/reconnect.rs` | `queries::bidi_transport::tests::reconnect::unretryable_reconnect_closes_every_lease` | Backend protocol; native test | `API-REQ-075` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/delivery.rs` | `queries::bidi_transport::tests::delivery::a_retire_remove_is_acked_without_closing_the_transport` | Backend protocol; native test | `API-REQ-075` |
| `crates/xmtp_api_backend/src/queries/bidi_transport_props.rs` | `queries::bidi_transport_props::ledger_delivers_exactly_the_asked_suffix_in_order` | Backend protocol; native test | `API-REQ-078` |
| `crates/xmtp_api_backend/src/queries/bidi_transport_props.rs` | `queries::bidi_transport_props::chunked_ledger_delivers_exactly_the_asked_suffix_in_order` | Backend protocol; native test | `API-REQ-078` |
| `crates/xmtp_api_grpc/src/grpc_client/client.rs` | `grpc_client::client::tests::metadata_test` | custom async | `API-REQ-079` |
| `crates/xmtp_api_grpc/src/grpc_client/native.rs` | `grpc_client::native::keepalive_tests::defaults_when_env_absent` | built-in sync; native-only module | `API-REQ-080` |
| `crates/xmtp_api_grpc/src/grpc_client/native.rs` | `grpc_client::native::keepalive_tests::env_overrides_are_applied` | built-in sync; native-only module | `API-REQ-080` |
| `crates/xmtp_api_grpc/src/grpc_client/native.rs` | `grpc_client::native::keepalive_tests::zero_tcp_keepalive_disables_it` | built-in sync; native-only module | `API-REQ-080` |
| `crates/xmtp_api_grpc/src/grpc_client/native.rs` | `grpc_client::native::keepalive_tests::invalid_values_fall_back_to_defaults` | built-in sync; native-only module | `API-REQ-080` |
| `crates/xmtp_api_grpc/src/streams/default.rs` | `streams::default::tests::test_successful_message_decoding` | rstest + custom async; empty/single/multiple cases | `API-REQ-081` |
| `crates/xmtp_api_grpc/src/streams/default.rs` | `streams::default::tests::test_error_propagation` | custom async | `API-REQ-081` |
| `crates/xmtp_api_grpc/src/streams/default.rs` | `streams::default::tests::stream_ends` | custom sync | `API-REQ-081` |
| `crates/xmtp_api_grpc/src/streams/multiplexed.rs` | `streams::multiplexed::tests::does_not_starve_s2` | custom sync | `API-REQ-082` |
| `crates/xmtp_api_grpc/src/streams/multiplexed.rs` | `streams::multiplexed::tests::polls_s2_in_between_s1` | custom sync | `API-REQ-082` |
| `crates/xmtp_api_grpc/src/streams/multiplexed.rs` | `streams::multiplexed::tests::ignores_items_after_s2_pending` | custom sync | `API-REQ-082` |
| `crates/xmtp_api_grpc/src/streams/multiplexed.rs` | `streams::multiplexed::tests::ends_when_s1_ends` | custom sync | `API-REQ-082` |
| `crates/xmtp_api_grpc/src/streams/multiplexed.rs` | `streams::multiplexed::tests::does_not_panic_on_polling_after_finish` | custom sync | `API-REQ-082` |
| `crates/xmtp_api_grpc/src/streams/non_blocking_stream.rs` | `streams::non_blocking_stream::tests::handles_err_on_establish` | custom sync | `API-REQ-083` |
| `crates/xmtp_api_grpc/src/streams/non_blocking_stream.rs` | `streams::non_blocking_stream::tests::happy_path_future` | custom sync | `API-REQ-083` |
| `crates/xmtp_api_grpc/src/streams/non_blocking_stream.rs` | `streams::non_blocking_stream::tests::establish_changes_state_to_started` | custom sync | `API-REQ-083` |
| `crates/xmtp_api_grpc/src/streams/try_from_item.rs` | `streams::try_from_item::tests::test_successful_conversions` | rstest + custom async; empty/single/3/5-item cases | `API-REQ-084` |
| `crates/xmtp_api_grpc/src/streams/try_from_item.rs` | `streams::try_from_item::tests::test_conversion_error_propagation` | custom async | `API-REQ-084` |
| `crates/xmtp_api_grpc/src/streams/try_from_item.rs` | `streams::try_from_item::tests::stream_can_finish` | custom sync | `API-REQ-084` |
| `crates/xmtp_api_grpc/src/streams/try_from_item.rs` | `streams::try_from_item::tests::happy_path` | custom sync | `API-REQ-084` |

## Phase 3 coverage

| File | Qualified test | Form / gates / cases | Requirements |
| --- | --- | --- | --- |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::publish_retries_identical_canonical_bytes_and_returns_metadata` | XMTP test; mock backend; parameter cases stay in one row | `API-REQ-001`, `API-REQ-007`, `P3-API-004` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::publish_hash_mismatch_is_terminal` | XMTP test; mock backend; parameter cases stay in one row | `P3-API-005` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::publish_size_errors_split_between_atomic_units` | XMTP test; mock backend; parameter cases stay in one row | `API-REQ-008`, `API-REQ-009`, `P3-API-019` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::one_rejected_atomic_unit_stops_without_splitting` | XMTP test; mock backend; parameter cases stay in one row | `P3-API-019` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::publish_chunks_measure_bytes_and_distinct_topics` | XMTP test; mock backend; parameter cases stay in one row | `API-REQ-008`, `API-REQ-009`, `P3-API-003` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::query_pages_three_times_with_independent_topic_cursors` | XMTP test; mock backend; parameter cases stay in one row | `API-REQ-002`, `API-REQ-006`, `RUST-REQ-093`, `P3-API-006` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::oversized_query_reduces_limit_before_splitting_topics` | XMTP test; mock backend; parameter cases stay in one row | `P3-API-019` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::query_rejects_has_more_without_progress` | XMTP test; mock backend; parameter cases stay in one row | `P3-API-006` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::key_packages_are_keyed_and_absence_is_explicit` | XMTP test; mock backend; parameter cases stay in one row | `API-REQ-005`, `P3-API-002` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::newest_respects_each_topic_limit` | XMTP test; mock backend; parameter cases stay in one row | `P3-API-007` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::inbox_lookup_chunks_and_preserves_duplicates_and_absence` | XMTP test; mock backend; parameter cases stay in one row | `API-REQ-003`, `P3-API-012` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::inbox_lookup_rejects_unknown_response_kind` | XMTP test; mock backend; parameter cases stay in one row | `P3-API-012` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::aborted_identity_publish_returns_conflict_without_retry` | XMTP test; mock backend; parameter cases stay in one row | `API-REQ-001`, `P3-API-008` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::get_does_not_retry_not_found` | XMTP test; mock backend; parameter cases stay in one row | `P3-API-008` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::group_decoder_keeps_payload_and_envelope_hashes_separate` | XMTP test; mock backend; parameter cases stay in one row | `P3-STR-011` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::welcome_decoder_retains_pointer_payload` | XMTP test; mock backend; parameter cases stay in one row | `P3-STR-011` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::query_splits_at_the_topic_limit` | XMTP test; mock backend; parameter cases stay in one row | `P3-API-007` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::newest_size_errors_split_until_one_topic` | XMTP test; mock backend; parameter cases stay in one row | `P3-API-019` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::signature_checks_chunk_and_keep_result_order` | XMTP test; mock backend; parameter cases stay in one row | `P3-API-007` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::single_publish_retries_resource_exhausted_with_backoff` | XMTP test; mock backend; parameter cases stay in one row | `API-REQ-007`, `P3-API-009` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::minimum_query_retries_resource_exhausted_with_backoff` | XMTP test; mock backend; parameter cases stay in one row | `API-REQ-007`, `P3-API-009` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::minimum_query_size_errors_remain_terminal` | XMTP test; mock backend; parameter cases stay in one row | `P3-API-019` |
| `crates/xmtp_api/src/tests/mod.rs` | `tests::invalid_input_returns_invalid_request_without_rpc` | XMTP test; mock backend; parameter cases stay in one row | `P3-API-008` |
| `crates/xmtp_api/src/tests/integration.rs` | `tests::integration::backend_round_trip_covers_five_kinds_paging_and_absent_key` | XMTP async; Docker backend; exact payloads, metadata, paging, absence, and duplicate publish | `API-REQ-001`, `API-REQ-002`, `API-REQ-004`, `API-REQ-005`, `API-REQ-006`, `API-REQ-009`, `P3-API-018` |
| `crates/xmtp_api/src/tests/limits.rs` | `tests::limits::read_topic_boundaries` | XMTP async; rstest; Docker backend; boundary and one-past cases; query row cap is discovered at runtime | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits.rs` | `tests::limits::publish_topic_boundary` | XMTP async; rstest; Docker backend; boundary and one-past cases; query row cap is discovered at runtime | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits.rs` | `tests::limits::publish_envelope_count_has_no_separate_cap` | XMTP async; rstest; Docker backend; boundary and one-past cases; query row cap is discovered at runtime | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits.rs` | `tests::limits::publish_byte_boundary` | XMTP async; rstest; Docker backend; boundary and one-past cases; query row cap is discovered at runtime | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits.rs` | `tests::limits::envelope_byte_boundary` | XMTP async; rstest; Docker backend; boundary and one-past cases; query row cap is discovered at runtime | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits.rs` | `tests::limits::inbox_identifier_boundary` | XMTP async; rstest; Docker backend; boundary and one-past cases; query row cap is discovered at runtime | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits.rs` | `tests::limits::scw_signature_boundary` | XMTP async; rstest; Docker backend; boundary and one-past cases; query row cap is discovered at runtime | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits.rs` | `tests::limits::query_row_clamp_boundary` | XMTP async; rstest; Docker backend; boundary and one-past cases; query row cap is discovered at runtime | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits.rs` | `tests::limits::static_topic_boundary` | XMTP async; rstest; Docker backend; boundary and one-past cases; query row cap is discovered at runtime | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits.rs` | `tests::limits::identity_entry_boundary` | XMTP async; rstest; Docker backend; boundary and one-past cases; query row cap is discovered at runtime | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits.rs` | `tests::limits::response_byte_boundary` | XMTP async; rstest; Docker backend; boundary and one-past cases; query row cap is discovered at runtime | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits/native.rs` | `tests::limits::native::limit_status_does_not_reopen_the_stream` | Native XMTP async; rstest; HTTP/2 asserts queue-then-admit; token-bucket cases require a burst before refill | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits/native.rs` | `tests::limits::native::update_entry_boundary` | Native XMTP async; rstest; HTTP/2 asserts queue-then-admit; token-bucket cases require a burst before refill | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits/native.rs` | `tests::limits::native::token_bucket_boundary` | Native XMTP async; rstest; HTTP/2 asserts queue-then-admit; token-bucket cases require a burst before refill | `P3-TST-002` |
| `crates/xmtp_api/src/tests/limits/native.rs` | `tests::limits::native::http2_stream_boundary` | Native XMTP async; rstest; HTTP/2 asserts queue-then-admit; token-bucket cases require a burst before refill | `P3-TST-002` |
| `crates/xmtp_api_backend/src/endpoints/backend/mod.rs` | `endpoints::backend::tests::endpoint_paths_match_backend_services` | XMTP test; one table for six unary paths and SubscribeStatic; bidi path stays in its transport test | `API-REQ-010` |
| `crates/xmtp_api_backend/src/queries/stream/extractor.rs` | `queries::stream::extractor::tests::preserves_order_and_all_errors` | XMTP async; empty input, order, decode errors, and wire errors | `API-REQ-051` |
| `crates/xmtp_api_backend/src/queries/stream/extractor.rs` | `queries::stream::extractor::tests::empty_stream_finishes` | XMTP async; empty input, order, decode errors, and wire errors | `API-REQ-051` |
| `crates/xmtp_api_backend/src/streams/tests.rs` | `streams::tests::id_only_subscription_starts_after_newest_cursor_without_a_gap` | XMTP async; scripted static streams | `P3-STR-010` |
| `crates/xmtp_api_backend/src/streams/tests.rs` | `streams::tests::static_subscriptions_split_at_the_topic_limit` | XMTP async; scripted static streams | `P3-STR-010` |
| `crates/xmtp_api_backend/src/streams/tests.rs` | `streams::tests::empty_subscription_does_not_open_a_wire_or_finish` | XMTP async; scripted static streams | `P3-STR-010` |
| `crates/xmtp_api_backend/src/streams/tests.rs` | `streams::tests::static_stream_surfaces_bad_envelopes` | XMTP async; scripted static streams | `P3-STR-010` |
| `crates/xmtp_api_backend/src/streams/tests.rs` | `streams::tests::three_silent_intervals_end_the_stream_with_a_retryable_error` | XMTP async; scripted static streams | `P3-STR-010` |
| `crates/xmtp_api_backend/src/streams/tests.rs` | `streams::tests::silent_second_wire_ends_the_complete_subscription` | XMTP async; scripted static streams | `P3-STR-010` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/catch_up.rs` | `queries::bidi_transport::tests::catch_up::target_zero_completes_only_after_applied` | Native XMTP async; scripted backend peer | `P3-STR-002` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/catch_up.rs` | `queries::bidi_transport::tests::catch_up::target_equal_to_floor_needs_no_delivery` | Native XMTP async; scripted backend peer | `P3-STR-002` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/catch_up.rs` | `queries::bidi_transport::tests::catch_up::a_topic_absent_from_targets_inherits_the_registration` | Native XMTP async; scripted backend peer | `P3-STR-003` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/catch_up.rs` | `queries::bidi_transport::tests::catch_up::two_holders_keep_independent_monotonic_floors` | Native XMTP async; scripted backend peer | `API-REQ-064`, `API-REQ-072`, `API-REQ-076`, `P3-STR-004` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/catch_up.rs` | `queries::bidi_transport::tests::catch_up::lower_cursor_readd_routes_queued_messages_at_applied_boundaries` | Native XMTP async; scripted backend peer | `API-REQ-071`, `P3-STR-004` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/limits.rs` | `queries::bidi_transport::tests::limits::a_lease_cannot_push_the_wire_past_the_topic_limit` | Native XMTP async; scripted backend peer | `P3-STR-013`, `P3-TST-002` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/coalescing.rs` | `queries::bidi_transport::tests::coalescing::queued_leases_coalesce_during_dial_and_deliver_once` | Native XMTP async; scripted backend peer | `P3-STR-001` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/catch_up.rs` | `queries::bidi_transport::tests::catch_up::unknown_applied_warns_without_disturbing_delivery` | Native XMTP async; scripted backend peer | `P3-STR-001` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/coalescing.rs` | `queries::bidi_transport::tests::coalescing::coalescing_keeps_limits_boundaries_and_ack_ids` | Native XMTP async; scripted backend peer | `P3-STR-001` |
| `crates/xmtp_api_backend/src/queries/bidi_transport/tests/coalescing.rs` | `queries::bidi_transport::tests::coalescing::coalescing_commits_ack_ids_only_after_wire_acceptance` | Native XMTP async; scripted backend peer | `P3-STR-001` |
