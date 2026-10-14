# Backend unit, RPC, storage, replica, and HTTPS streaming tests, and the SQLx
# metadata check. The check phase runs through `services-test` of the storage
# services (nix/services: a PostgreSQL primary, a streaming replica, and
# VersityGW) on free loopback ports in the build sandbox. The tests start the
# backend from source, so the services start no backend.
{
  xmtp,
  lib,
  stdenv,
  writeShellScript,
  postgresql_18,
  sqlx-cli,
  cacert,
  services,
}:
let
  inherit (xmtp) base;
  root = ./../..;
  darwin = stdenv.hostPlatform.isDarwin;
  rust = xmtp.craneLib.overrideToolchain xmtp.hostToolchain;
  src = xmtp.filesets.mkClosureSource rust {
    roots = [ (root + /apps/backend) ];
    dev = true;
  };
  # Compilation reads the committed `.sqlx` metadata. Only the check phase
  # uses a database.
  commonArgs = base.commonArgs // {
    SQLX_OFFLINE = "true";
  };
  test = "cargo test --locked -p xmtp_backend";
  # A replica test connects to its new database on the replica at once. WAL
  # from concurrent tests delays replica replay, so Linux runs these tests
  # last, after the replica has replayed all WAL. On Darwin the replica replays
  # a new database more slowly than these tests allow, so Darwin skips them.
  replicaTests = "stream::tests::replica::";
  # A Nix build user on Darwin cannot use the system TLS trust service. These
  # tests trust a test certificate through it. Linux runs them.
  trustTests = [
    "push::channel::http::tests::response_classes_and_redirects_use_the_provider_contract"
    "push::channel::http::tests::send_time_domain_allowlist_rechecks_registered_destinations"
    "push::channel::http::tests::tls_requests_use_pinned_dns_and_fresh_signed_headers_without_secrets"
    "push::channel::http::tests::webhook_logs_do_not_contain_recipient_fields_or_signing_keys"
    "push::tests::lifecycle::real_https_all_gone_deletes_recipient_after_max_attempts"
    "push::tests::lifecycle::real_https_retries_gone_then_success_without_deleting_recipient"
    "server::tests::transport::https_ingress::https_passthrough_preserves_streaming_headers_and_status_details"
  ];
  skips = lib.concatMapStringsSep " " (name: "--skip ${name}") (
    [ replicaTests ] ++ lib.optionals darwin trustTests
  );
  # Four test threads bound the database connections, as in the recipe. The
  # SQLx check touches backend sources, so it runs after the tests.
  run = writeShellScript "backend-tests-run" ''
    set -euo pipefail
    RUST_TEST_THREADS=4 ${test} -- ${skips}
    ${lib.optionalString (!darwin) ''
      lsn=$(psql "$DATABASE_URL" -Atc 'SELECT pg_current_wal_lsn()')
      replayed() {
        [[ $(psql "$XMTP_REPLICA_URL" -Atc "SELECT pg_last_wal_replay_lsn() >= '$lsn'") == t ]]
      }
      for _ in $(seq 600); do replayed && break; sleep 0.1; done
      replayed
      ${test} --lib ${replicaTests}
    ''}
    cargo sqlx migrate run --source apps/backend/migrations
    cd apps/backend
    cargo sqlx prepare --check -- --all-targets
  '';
  # The SQLx check compiles the backend in check mode. Cache that mode too.
  cargoArtifacts = base.mkCargoArtifacts rust (
    commonArgs
    // {
      buildPhaseCargoCommand = ''
        ${test} --no-run
        cargo check --locked --all-targets -p xmtp_backend
      '';
    }
  );
in
rust.mkCargoDerivation (
  commonArgs
  // {
    inherit src cargoArtifacts;
    cargoVendorDir = base.mkCargoVendorDir rust;
    pname = "xmtp-backend-tests";
    version = xmtp.mkVersion rust;
    buildPhaseCargoCommand = "${test} --no-run";
    doCheck = true;
    # psql waits for replica replay. The backend's HTTP clients read the
    # trust store that cacert sets; a Linux sandbox has none.
    nativeCheckInputs = [
      postgresql_18
      sqlx-cli
      cacert
    ];
    checkPhaseCargoCommand = services.checkCommand run;
    doInstallCargoArtifacts = false;
    # A check result must name no store path, so it substitutes on a hit.
    allowedReferences = [ ];
  }
  // services.sandbox
)
