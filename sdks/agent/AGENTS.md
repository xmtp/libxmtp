# XMTP Agent SDK

```bash
just js test-agent-sdk-ci
```

## Stream startup

- `Agent.start()` starts native streams without a separate network sync by default. Core owns the finite network recovery budget from startup.
- The `start` event means the local stream pumps are ready. The network can still be offline.
- Conversation events exclude groups already stored locally. A pending Welcome produces an event when the client first discovers that group.
- An explicit `disableSync: false` requests the legacy pre-sync operation. That operation has its own bounded sync deadline before native streaming starts.
- Terminal stream errors stop the current generation. The caller can use `start()` again for a fresh budget on the same client.
