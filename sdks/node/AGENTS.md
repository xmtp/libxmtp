# XMTP Node SDK

```bash
just js test-node-sdk-ci
just js test-node-sdk-ci test/createBackend.test.ts # one file
just js test-node-sdk-ci -t create # tests with matching titles
```

`streamRecovery.test.ts` owns its TCP proxies. To test actual graceful backend
shutdown and restart, build the backend and set its absolute binary path:

```bash
dev/nix-shell 'just backend build'
XMTP_RECOVERY_BACKEND_BINARY="$(realpath result/bin/xmtp-backend)" dev/nix-shell 'just js test-node-sdk-ci test/streamRecovery.test.ts'
```

The drain cases then start a separate backend process on private ports. They
use the worktree database and leave the shared backend running. Without the
binary path, those cases test TCP EOF. Neither mode proves a rolling deployment
through an external load balancer. Run these tests one process at a time.
