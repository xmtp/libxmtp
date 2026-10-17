# xmtp_backend_config

The backend configuration file: its types, defaults, environment references,
and every check that the document and the environment can decide. The CONF
spec states the contract (`docs/specs/CONF-backend-configuration.md`). The
backend (`apps/backend`) re-exports this crate as `xmtp_backend::config` and
keeps everything that turns configuration into behaviour: the published
`GetConfiguration` response (`src/published.rs`), signing-key parsing
(`src/auth/keys.rs`), and the clock checks against the database.

## Commands

```bash
dev/nix-shell 'just check crate xmtp_backend_config'
dev/nix-shell 'just test crate xmtp_backend_config'
dev/nix-shell 'just backend schema'   # regenerate docs/schemas/backend-v1.json
```

## Rules

- A check that needs wire types, credential crypto, or I/O, or that builds a
  live object, belongs in the backend. A check that takes a runtime fact as an
  argument, such as `RetentionConfig::validate_at`, belongs here.
- The normal dependencies stay light. Never add `xmtp_proto`, `xmtp_id`,
  `xmtp_mls_common`, `xmtp_logging`, `xmtp_attachments_server`, `xmtp_common`,
  `xmtp_cryptography`, `openmls`, `jsonwebtoken`, a key crate, an `aws-*`
  crate, `tonic`, `sqlx`, `tokio`, `reqwest`, or `xmtp-workspace-hack`.
  `xmtp_common` is a dev-dependency for the test macro only. Subsystem crates
  depend on this crate, never the reverse: `xmtp_attachments_server` builds its
  storage target from `attachments`.
- Doc comments on configuration types and fields are schema descriptions.
  Changing one changes `docs/schemas/backend-v1.json`; regenerate it with
  `just backend schema` and commit both. The file's key order comes from
  schemars' `preserve_order` feature, and a test compares its exact text.
- Refusals name the key and never include a value, because a value may come
  from the environment.

## Tests

- `src/tests/example.rs` reads the field table, the section 3 defaults, and the
  example file from the CONF spec. `src/tests/releases/` holds each release's
  copy of the example file; a copy never changes once committed, and taplo
  skips it (`nix/fmt.nix`).
- Tests that set environment variables run one test in a child process with
  `tests::child_test(module_path!(), name)` and check it with
  `tests::assert_one_test_passed`.
