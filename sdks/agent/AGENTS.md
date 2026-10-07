# XMTP Agent SDK

```bash
dev/nix-shell 'just js test-agent-sdk-ci'
```

## Stream startup

- `Agent.start()` starts native streams without a separate network sync. Core owns the finite network recovery budget from startup.
- The `start` event means the local stream pumps are ready. The network can still be offline.
- Conversation events exclude groups already stored locally. A pending Welcome produces an event when the client first discovers that group.
- Terminal stream errors stop the current generation. Cleanup waits for a pending stream open and closes its result before error middleware runs. The caller can use `start()` again for a fresh budget on the same client, including from error middleware.

`src/migration.test.ts` checks the migration re-export from the normal Agent
package against the normal Node package.
