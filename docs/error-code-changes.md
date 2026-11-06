# Error code changes in the xmtp_mls split

The `error_code()` values below existed in `docs/error_glossary.md` at the
first split commit (`fb3163f82`, #4860) and no longer exist. Each row names
the code that the same failure returns now. The SDK's public error kinds,
codes and retry flags did not change; these are the Rust `ErrorCode` values
of `xmtp_mls` and its crates.

| Removed code | Replacement |
| --- | --- |
| `ClientError::Generic` | `ClientError::MessageDecodeFailed` for a message that fails to decode; `IdentityUpdateError::InboxNotFound` for a membership lookup of an unknown inbox |
| `GroupError::Client` | `GroupError::Blocked` for group work on a blocked connection; `GroupError::IdentityUpdate` for identity reads inside group work |
| `GroupError::DeviceSync` | `GroupError::Storage` for group work that enqueues missing installations |
| `GroupError::LocalEvent` | Removed; nothing returned it |
| `IdentityError::IdentityUpdate` | Removed; nothing returned it |

Some routes also return a different code while their old code still exists
for other routes, so those codes are not listed above. For example, a
blocked connection during a server-configuration fetch reported
`ClientError::BackendMismatch` and now reports
`BlockedConnection::BackendMismatch`. #4863 lists every changed route.
