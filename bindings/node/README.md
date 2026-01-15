# Node bindings for the libXMTP rust library

> [!INFO]
> These bindings are not intended to be used directly, use the associated SDK instead.

## Code Organization

The Rust source code follows a modular structure organized by domain:

```
src/
├── lib.rs                 # Library entry point, ErrorWrapper utility
├── client/                # Client struct and methods (split by concern)
├── conversations/         # Conversations methods (list, create, stream)
├── conversation/          # Conversation methods (split by concern)
├── content_types/         # Content type definitions (one file per type)
│   ├── mod.rs             # ContentType enum and re-exports
│   ├── text.rs, reaction.rs, reply.rs, attachment.rs, ...
├── messages/              # Message types and utilities
│
└── [root-level files]   # Shared types, enums, and utils used across modules
```

### Where to Find Code

- **Client methods**: `src/client/` - Find the file matching the concern (signatures, consent, etc.)
- **Creating clients**: `src/client/create_client.rs` and `src/client/options.rs`
- **Listing/creating conversations**: `src/conversations/`
- **Single conversation operations**: `src/conversation/`
- **Group-specific operations**: `src/conversation/group/`
- **Dm-specific operations**: `src/conversation/dm.rs`
- **Content type definitions**: `src/content_types/`
- **Message types**: `src/messages/`
- **Shared types/enums**: Root-level files in `src/`

### Where to Add New Code

- **New Client method**: Add to existing file in `src/client/` by concern, or create new file if new concern
- **New Conversations method**: Add to `src/conversations/mod.rs` or appropriate submodule
- **New Conversation method**: Add to `src/conversation/mod.rs` or appropriate submodule
- **New content type**: Create `src/content_types/<type_name>.rs` and add to `mod.rs`
- **New shared type/enum**: Create root-level file in `src/`

## Useful commands

- `yarn`: Installs all dependencies (required before building)
- `yarn build`: Build a release version of the Node bindings for the current platform
- `yarn lint`: Run cargo clippy and fmt checks
- `yarn test`: Run the test suite on Node

## Testing

Before running the test suite, a local XMTP node must be running. This can be achieved by running `./dev/up` at the root of this repository. Docker is required.
