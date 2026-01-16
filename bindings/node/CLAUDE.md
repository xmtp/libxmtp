# Node Bindings - Claude Assistant Context

This file provides context for Claude Code to understand the node bindings crate structure and development workflows.

## Crate Overview

This crate (`bindings_node`) provides Node.js bindings for libxmtp using NAPI-RS. It exposes the core XMTP functionality to JavaScript/TypeScript applications.

## Code Organization

The crate follows a domain-driven modular structure:

### Core Modules

| Module           | Purpose                                                 |
| ---------------- | ------------------------------------------------------- |
| `client/`        | Client struct and all its methods, split by concern     |
| `conversations/` | Conversations collection - listing, creating, streaming |
| `conversation/`  | Conversation methods, split by concern                  |
| `content_types/` | Content type definitions (text, reaction, reply, etc.)  |
| `messages/`      | Message struct and related types                        |

### Key Files

| File                      | Contains                                   |
| ------------------------- | ------------------------------------------ |
| `lib.rs`                  | Crate entry point, `ErrorWrapper` utility  |
| `client/mod.rs`           | `Client` struct definition, core methods   |
| `client/create_client.rs` | `create_client()` function, client builder |
| `client/options.rs`       | `ClientOptions` configuration              |
| `conversations/mod.rs`    | `Conversations` struct, core methods       |
| `conversation/mod.rs`     | `Conversation` struct, core methods        |
| `messages/mod.rs`         | `Message` structs and enums                |

### Pattern: Methods Split by Concern

Large structs like `Client` and `Conversation` have their `impl` blocks split across multiple files:

```
client/
├── mod.rs           # Client struct + core methods
├── consent_state.rs # impl Client { consent methods }
├── signatures.rs    # impl Client { signature methods }
└── ...
```

Each file adds methods to the same struct via `impl Client { ... }` blocks. This keeps files focused and manageable.

### Pattern: conversation/ vs conversations/

- `conversation/` = methods on a **single** Conversation (send message, get members, etc.)
- `conversations/` = methods on the **collection** (list, create, stream conversations)

## Common Tasks

### Adding a New Client Method

1. Identify the concern (consent, signatures, identity, etc.)
2. Find or create the appropriate file in `src/client/`
3. Add the method in an `impl Client` block with `#[napi]` attribute

```rust
// src/client/my_feature.rs
use napi_derive::napi;
use crate::client::Client;

#[napi]
impl Client {
    #[napi]
    pub async fn my_new_method(&self) -> napi::Result<()> {
        // implementation
        Ok(())
    }
}
```

4. Add `mod my_feature;` to `src/client/mod.rs`

### Adding a New Conversation Method

Same pattern as Client - add to `src/conversation/` in appropriate file.

### Adding a New Content Type

1. Create `src/content_types/<type_name>.rs`
2. Define the struct with `#[napi(object)]`
3. Add `pub mod <type_name>;` to `src/content_types/mod.rs`
4. Add variant to `ContentType` enum in `mod.rs`
5. Update `DecodedMessageContentType`, `DecodedMessageContentInner`, and `DecodedMessageContent`
6. Add encoder function if content type should be sent by clients
7. Add function to expose `ContentTypeId` struct if content type will be read by clients

### Adding a Shared Type

Create a root-level file in `src/` for types used across multiple modules.

## Development Commands

```bash
# Install dependencies (required first)
yarn

# Build release version
yarn build

# Run linting (clippy + fmt)
yarn lint

# Run tests (requires local XMTP node via ./dev/up)
yarn test

# Check TypeScript file formatting=
yarn format:check

# Format TypeScript files
yarn format
```

## Important Patterns

### Error Handling

Use `ErrorWrapper` to convert Rust errors to NAPI errors:

```rust
use crate::ErrorWrapper;

self.inner_client
    .some_method()
    .await
    .map_err(ErrorWrapper::from)?;
```

### NAPI Attributes

- `#[napi]` on struct/impl - exports to JavaScript
- `#[napi(object)]` - plain object (not a class)
- `#[napi(getter)]` - property getter
- `#[napi]` on method - exported method

### BigInt for Timestamps

Use `napi::bindgen_prelude::BigInt` for nanosecond timestamps to avoid precision loss.

### Cloning Inner Types

`Conversation` uses a pattern where `create_mls_group()` creates a new `MlsGroup` from stored data, since the inner group isn't clonable in a way that makes sense for NAPI.

## Testing

Tests are in `test/` directory as TypeScript files using Vitest:

- `Client.test.ts`
- `Conversation.test.ts`
- `Conversations.test.ts`

Run tests with `yarn test`. Requires local XMTP node (start with `./dev/up` from repo root).

## Key Dependencies

- `napi` / `napi-derive` - Node.js bindings
- `xmtp_mls` - Core MLS implementation
- `xmtp_db` - Database layer
- `xmtp_proto` - Protocol buffer definitions

## File Naming Convention

- `mod.rs` - Module entry point, contains primary struct
- `<concern>.rs` - Additional impl blocks for that concern
- `<type>.rs` in content_types/ - Individual content type definitions
