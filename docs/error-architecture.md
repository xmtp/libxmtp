# Error code architecture

This document explains how the error code system works internally, how to add new error codes, and how codes propagate through FFI bindings.

> **SDK consumers:** see [error-glossary.md](error-glossary.md) for the complete list of error codes and how to parse them on each platform.

## How it works

Every error type in LibXMTP implements the `ErrorCode` trait, defined in `crates/xmtp_common/src/error_code.rs`:

```rust
pub trait ErrorCode: std::error::Error {
    /// Returns the unique error code for this error.
    /// Format: "TypeName::VariantName" for enums, "TypeName" for structs.
    fn error_code(&self) -> &'static str;
}
```

The trait is derived automatically using a proc macro (`#[derive(ErrorCode)]`). The derive macro lives in `crates/xmtp_macro/src/lib.rs`.

## The `#[derive(ErrorCode)]` macro

### Basic usage

Add `ErrorCode` to your derive list alongside `thiserror::Error`:

```rust
use thiserror::Error;
use xmtp_common::ErrorCode;

#[derive(Debug, Error, ErrorCode)]
pub enum MyError {
    #[error("something went wrong")]
    SomethingWrong,

    #[error("invalid input: {0}")]
    InvalidInput(String),
}
```

This generates:
- `MyError::SomethingWrong` -> `"MyError::SomethingWrong"`
- `MyError::InvalidInput("bad")` -> `"MyError::InvalidInput"`

### Struct errors

For struct errors (not enums), the code is just the type name:

```rust
#[derive(Debug, Error, ErrorCode)]
#[error("multiple receive errors")]
pub struct ReceiveErrors {
    pub errors: Vec<GroupError>,
}
```

This generates code `"ReceiveErrors"` for all instances.

### `#[error_code(inherit)]` -- delegating to inner errors

When an error variant wraps another error that implements `ErrorCode`, use `inherit` to delegate the code to the inner error. This is the key mechanism that lets SDK consumers always see the most specific (leaf) error code.

```rust
#[derive(Debug, Error, ErrorCode)]
pub enum GroupError {
    #[error("storage error: {0}")]
    #[error_code(inherit)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    #[error_code(inherit)]
    NotFound(#[from] NotFound),

    #[error("Group is inactive")]
    GroupInactive,
}
```

With `inherit`, the error code for `GroupError::Storage(StorageError::NotFound(...))` is `"StorageError::NotFound"`, not `"GroupError::Storage"`.

**Requirements for `inherit`:**
- The variant must have exactly one field (named or unnamed)
- The inner type must implement `ErrorCode`

### `#[error_code("CustomCode")]` -- backwards compatibility

If you need to rename a variant but keep the old error code for backwards compatibility:

```rust
#[derive(Debug, Error, ErrorCode)]
pub enum MyError {
    #[error("new variant name")]
    #[error_code("MyError::OldVariantName")]
    NewVariantName,
}
```

This returns `"MyError::OldVariantName"` instead of `"MyError::NewVariantName"`.

### `#[error_code(remote = "...")]` -- external types

For types defined in external crates (like `xmtp_cryptography`) that you cannot derive `ErrorCode` on directly, use the `remote` pattern. Define a mirror enum and point to the real type:

```rust
#[derive(xmtp_common::ErrorCode)]
#[error_code(remote = "xmtp_cryptography::signature::SignatureError")]
enum SignatureError {
    BadAddressFormat(()),
    BadSignatureFormat(()),
    BadSignature { addr: String },
    Signer(()),
    Unknown,
}
```

This implements `ErrorCode` for the external `xmtp_cryptography::signature::SignatureError` type. The mirror enum's variants must match the external type's variants.

All remote implementations are centralized in `crates/xmtp_common/src/error_code.rs` in the `cryptography_error_codes` module.

## How codes propagate through FFI bindings

All three binding layers format errors as `[ErrorCode] human-readable message`.

### Mobile (UniFFI)

In `bindings/mobile/src/lib.rs`:

```rust
#[derive(thiserror::Error, Debug, ErrorCode)]
pub enum GenericError {
    #[error("Client error: {0}")]
    #[error_code(inherit)]
    Client(#[from] ClientError),
    // ... most variants inherit
}

#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum FfiError {
    Error(GenericError),
}

impl std::fmt::Display for FfiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FfiError::Error(e) => write!(f, "[{}] {}", e.error_code(), e),
        }
    }
}
```

### Node.js (NAPI)

In `bindings/node/src/lib.rs`:

```rust
pub struct ErrorWrapper<E>(pub E) where E: ErrorCode;

impl<T: ErrorCode> From<ErrorWrapper<T>> for napi::bindgen_prelude::Error {
    fn from(e: ErrorWrapper<T>) -> napi::bindgen_prelude::Error {
        let code = e.0.error_code();
        Error::from_reason(format!("[{}] {}", code, e.0))
    }
}
```

### WASM

In `bindings/wasm/src/lib.rs`:

```rust
pub struct ErrorWrapper<E>(pub E) where E: ErrorCode;

impl<T: ErrorCode> From<ErrorWrapper<T>> for JsError {
    fn from(e: ErrorWrapper<T>) -> JsError {
        let code = e.0.error_code();
        let js_error = JsError::new(&format!("[{}] {}", code, e.0));
        let js_value: JsValue = js_error.clone().into();
        // Also set `code` as a JS property on the error object
        let _ = js_sys::Reflect::set(
            &js_value,
            &JsValue::from_str("code"),
            &JsValue::from_str(code),
        );
        js_error
    }
}
```

## Adding a new error code: step by step

### 1. Add a new variant to an existing error enum

```rust
#[derive(Debug, Error, ErrorCode)]
pub enum GroupError {
    // existing variants...

    #[error("my new error condition")]
    MyNewCondition,
}
```

This automatically generates the code `"GroupError::MyNewCondition"`.

### 2. Add a variant that wraps another error

```rust
#[derive(Debug, Error, ErrorCode)]
pub enum GroupError {
    // existing variants...

    #[error("some other error: {0}")]
    #[error_code(inherit)]
    SomeOther(#[from] SomeOtherError),
}
```

SDK consumers will see `SomeOtherError::*` codes, not `GroupError::SomeOther`.

### 3. Create a new error enum

```rust
use thiserror::Error;
use xmtp_common::ErrorCode;

#[derive(Debug, Error, ErrorCode)]
pub enum MyNewError {
    #[error("condition A")]
    ConditionA,

    #[error("condition B: {0}")]
    ConditionB(String),

    #[error("wrapped storage error: {0}")]
    #[error_code(inherit)]
    Storage(#[from] StorageError),
}
```

Then wire it into the parent error (e.g. `GroupError`, `ClientError`) with `#[error_code(inherit)]` so the codes propagate up to the FFI layer.

### 4. Add a new remote error code (external crate)

Add a new mirror enum in `crates/xmtp_common/src/error_code.rs`:

```rust
#[derive(xmtp_common::ErrorCode)]
#[error_code(remote = "some_crate::SomeError")]
enum SomeError {
    VariantA(()),
    VariantB,
}
```

### 5. Update the glossary

After adding new error codes, update `docs/error-glossary.md` with the new codes, descriptions, common causes, and retryability.

## Design principles

1. **Error codes are stable.** Never change an existing error code string. If you rename a variant, use `#[error_code("OldType::OldVariant")]` to preserve the code.

2. **Human messages can change.** The `#[error("...")]` message is not part of the API contract. SDK consumers should only match on codes.

3. **Inherit by default for wrappers.** If a variant wraps another `ErrorCode`-implementing type, use `#[error_code(inherit)]`. This ensures SDK consumers see the most specific error.

4. **Leaf codes win.** Through inheritance, the deepest (most specific) error code bubbles up through all wrapper layers.

5. **Consistent formatting.** All bindings use `[Code] message` format. The WASM binding additionally sets a `.code` JS property.

## Testing error codes

Tests live alongside the `ErrorCode` trait in `crates/xmtp_common/src/error_code.rs` and in each binding's `src/lib.rs`.

```rust
#[test]
fn test_enum_error_code() {
    let err = StorageError::Connection;
    assert_eq!(err.error_code(), "StorageError::Connection");
}

#[test]
fn test_inherited_error_code() {
    let err = GroupError::Storage(StorageError::Connection);
    assert_eq!(err.error_code(), "StorageError::Connection");
}

#[test]
fn test_custom_error_code() {
    let err = RenamedError::NewVariantName;
    assert_eq!(err.error_code(), "RenamedError::OldVariantName");
}
```

When adding new error types, add tests verifying:
- Each variant produces the expected code string
- Inherited variants produce the inner error's code
- Custom codes return the specified string

## Key files

| File | Purpose |
|------|---------|
| `crates/xmtp_common/src/error_code.rs` | `ErrorCode` trait definition, remote impls, tests |
| `crates/xmtp_macro/src/lib.rs` | `#[derive(ErrorCode)]` proc macro implementation |
| `bindings/mobile/src/lib.rs` | `GenericError`, `FfiError`, `parse_xmtp_error` |
| `bindings/node/src/lib.rs` | `ErrorWrapper` for NAPI |
| `bindings/wasm/src/lib.rs` | `ErrorWrapper` for WASM with `.code` property |
| `docs/error-glossary.md` | SDK consumer-facing error code reference |
