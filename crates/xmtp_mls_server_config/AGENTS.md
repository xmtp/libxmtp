# xmtp_mls_server_config

The deployment's published configuration (`server_configuration`) and the
local deployment record (`storage_location`). The refresh worker and
preflight live in `xmtp_mls_core`. `xmtp_mls` re-exports both modules at
their old paths.

```sh
dev/nix-shell 'just backend up'
dev/nix-shell 'just test crate xmtp_mls_server_config'
```

- `test-hooks` items change runtime behavior; enable them only from tests.
  `xmtp_mls` forwards `test-utils` and `test-hooks`.
- The `server_config` test target builds clients through the `xmtp_mls`
  dev-dependency. Tests that read private state stay unit tests.
