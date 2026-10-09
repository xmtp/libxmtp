# Chronological history pages

`Group.messageHistoryPage` and `Dm.messageHistoryPage` read retained Published
messages. They order by sent time, then immutable local delivery sequence.
The default direction is ascending and the default limit is 50. Use descending
order to read the newest page or older history.

The records are:

- `MessageHistoryPosition`: exact `sentAt` and opaque `deliveryCursor`.
- `MessageHistoryPage`: `messages`, `firstPosition`, `lastPosition`, `hasMore`,
  and `skippedCount`.

Keep every cursor unchanged. The SDK validates its database identity and local
position. A saved position remains a valid query boundary after its message is
deleted. A whole-database restore or another database rejects that cursor.

## Select a page

The three arguments are existing `ListMessagesOptions`, optional `before`, and
optional `after`. Generated callers may omit all three.

- `before` selects strict tuple less-than. Use it for older descending pages.
- `after` selects strict tuple greater-than. Use it for newer ascending pages.
- Both bounds select an open interval. Equal or reversed bounds give an empty
  page after cursor validation.
- Explicit limits use the positive UInt32 domain. Zero is an input error.
- None or SentAt sort is accepted. InsertedAt is an input error.
- None or Published status is accepted. Failed and Unpublished are input errors.
- Existing kind, content, sender, time and expiry filters apply before the limit.

The new page method leaves existing `messages`, `countMessages`, and
`messageHistorySnapshot` behavior unchanged. It leaves reader leases and
acknowledgement progress unchanged.

## Continue through conversion loss

`firstPosition` and `lastPosition` describe consumed raw rows in output order.
They can exist when `messages` is empty. `skippedCount` reports consumed rows
that could not be converted. Show that state to the user.

Use `hasMore` to test whether another matching key exists. Continue from
`lastPosition` in the same direction. The extra key that establishes `hasMore`
is unconsumed and has no loaded base body. Its position is not a continuation.
A short or empty `messages` list is not an exhaustion test.

For example, a page can consume 50 rows and return zero readable messages. If
it has more rows, its SDK-issued `lastPosition` still lets the next page reach
the remaining readable messages.

## Retain the display order

Preserve the order returned by each SDK page. Use message IDs for identity
merges. Use SDK positions for query boundaries. Do not decode or sort cursor
strings. Sent time stays primary, so old backfills remain older even when their
local delivery sequence is newer. Equal-time order is client-local.

Pages are separate snapshots. Refresh affected loaded windows after late
arrival, expiry, archive restore, or lost events. Retain the query boundaries
and the visible anchor position so deletion can select surviving neighbors.

Pending and failed rows remain available through the existing status query.
A history page does not resend them.

## Query cost

The SDK resolves the physical groups of a stitched DM in the read transaction.
With limit L and K physical groups, it reads at most K*(L+1) candidate keys,
merges the ordered keys, and loads at most L base bodies. Reply and reaction
enrichment remains available. The query index covers group, sent time and
local delivery sequence.

The implementation and record declarations are in
[src/delivery/history.rs](src/delivery/history.rs). The database selector is in
[history_page.rs](../xmtp_db/src/encrypted_store/group_message/history_page.rs).
