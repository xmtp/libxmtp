# Errors

## Shape

One `thiserror` enum per concern; a module may hold several. Wrap sub-errors
with `#[from]`. Use `#[error(transparent)]` when the variant adds no context.
Never stringify an inner error.

```rust
// pattern, after crates/xmtp_mls/src/groups/error.rs
use xmtp_common::{ErrorCode, RetryableError, retryable};

#[derive(Debug, thiserror::Error, ErrorCode)]
pub enum GroupError {
    #[error(transparent)]
    #[error_code(inherit)]
    NotFound(#[from] NotFound),
    /// Durable processing did not meet the fixed network targets. May be retryable.
    #[error(transparent)]
    #[error_code(inherit)]
    StreamBarrier(#[from] BarrierError),
    /// Another operation holds the group lock. Retryable.
    #[error("group lock unavailable")]
    LockUnavailable,
}

impl RetryableError for GroupError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::NotFound(e) => e.is_retryable(),
            Self::StreamBarrier(e) => retryable!(e),
            Self::LockUnavailable => true,
        }
    }
}
```

## Stable codes: `#[derive(ErrorCode)]`

`error_code()` returns `"TypeName::VariantName"` for an enum and `"TypeName"`
for a struct. Derive it on every error that can cross an FFI boundary. Each
variant without `inherit` needs a `///` doc comment; the derive rejects a
missing one. State in the doc whether the variant is retryable. The glossary
copies the doc text verbatim.

| Attribute | On | Effect |
| --- | --- | --- |
| `#[error_code(inherit)]` | variant with one field | return the inner error's code |
| `#[error_code("Type::OldName")]` | variant or struct | keep a code after a rename; write the full string |
| `#[error_code(internal)]` | type | drop the type from the glossary; codes still generated |
| `#[error_code(remote = "path::Type")]` | mirror enum | implement `ErrorCode` for a foreign type (`crates/xmtp_common/src/error_code.rs`) |

After adding variants: `dev/nix-shell 'dev/gen-error-glossary'` regenerates
`docs/error_glossary.md`.

## Retryability: `RetryableError`

A trait, not a bool field. Implement it by hand and delegate to inner errors.
`retry_async!` honours it. `retryable!(e)` is `e.is_retryable()` with the trait
in scope.

Coverage is not uniform. `xmtp_db` errors derive `ErrorCode` and implement
`RetryableError`. `xmtp_id` has gaps (`SignerError`, `IdentityError`). Check
before assuming, and add the missing impl when a code must cross FFI.
`xmtp_common::time::Expired` implements `ErrorCode`; wrap it with `#[from]`.

## Bindings

Each surface has one wrapper that emits `"[{code}] {message}"`. Use it. Do not
format error strings by hand.

| Surface | Wrapper | Use |
| --- | --- | --- |
| node | `ErrorWrapper<E>` in `bindings/node/src/lib.rs` | `.map_err(ErrorWrapper::from)?` |
| wasm | `ErrorWrapper<E>` in `bindings/wasm/src/errors.rs` | `.map_err(ErrorWrapper::js)?`; keeps the JS `code` property |
| mobile | `FfiError` in `bindings/mobile/src/lib.rs` | `?` through `From<T: Into<GenericError>>` |

## Backend

`apps/backend/src/error.rs` is the typed database error. One
`From<Error> for Status` in `apps/backend/src/service/error.rs` maps it to
transport codes. Database helpers never return `Status`.
