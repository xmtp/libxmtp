# xmtp_events

Live, per-client event types, filters, buffer, and subscriptions.

## Commands

Run from the repository root through `dev/nix-shell`:

```bash
just check crate xmtp_events
just test crate xmtp_events
```

The crate must not depend on `xmtp_mls`. Each `EventBus` belongs to one client.
