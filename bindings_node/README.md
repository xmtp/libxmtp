# Node bindings for the libXMTP rust library

> [!CAUTION]
> These bindings are currently in alpha and under heavy development. The API is subject to change and it is not yet recommended for production use.

## Useful commands

- `yarn`: Installs all dependencies (required before building)
- `yarn build`: Build a release version of the Node bindings for the current platform
- `yarn lint`: Run cargo clippy and fmt checks
- `yarn test`: Run the test suite on Node

## Testing

Before running the test suite, a local XMTP node must be running. This can be achieved by running `./dev/up` at the root of this repository. Docker is required.
