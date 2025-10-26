# libxmtp
![XMTP](https://avatars.githubusercontent.com/u/82580170?s=48&v=4)

![https://github.com/xmtp/libxmtp/actions/workflows/lint.yml/badge.svg](https://github.com/xmtp/libxmtp/actions/workflows/lint.yml/badge.svg)
![Status](https://img.shields.io/badge/Project_status-Alpha-orange)

**The battle-tested Rust core powering decentralized messaging for Web3**

Build encrypted, wallet-to-wallet messaging into any app. No servers to maintain, no data to leak, no middlemen to trust.

```rust
// Send encrypted messages between any Ethereum addresses
let client = Client::create(wallet, env).await?;
let conversation = client.conversations()
    .create_group(vec![wallet_address])
    .await?;

conversation.send("gm! 🌅".as_bytes()).await?;
```

## Why libxmtp?

**🔐 True End-to-End Encryption** - Messages are encrypted before they leave your device. Even we can't read them.

**🛡️ Perfect Forward Secrecy** - Built on MLS (Messaging Layer Security). Each message uses unique keys, so past conversations stay secure even if current keys are compromised.

**🏗️ Decentralized by Design** - No central servers, no single points of failure. Messages flow through a distributed network.

**⚡ Wallet-Native** - Use your existing Ethereum wallet as your identity. No new accounts, no password recovery.

**📱 Universal Platform Support** - One codebase, everywhere. iOS, Android, React Native, Web, Node.js, and browsers via WebAssembly.

**🔧 Production Ready** - Powers messaging for thousands of users across mobile apps, web dapps, and desktop clients.

**🦀 Rust Foundation** - Memory-safe, reliable core with bindings for JavaScript, Swift, Kotlin, and more.

## Quick Start

```rust
use libxmtp::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize client with your wallet
    let client = Client::create(your_wallet, Environment::Production).await?;

    // Start a conversation
    let conversation = client.conversations()
        .create_dm(recipient_address)
        .await?;

    // Send encrypted message
    conversation.send("Hello, decentralized world!".as_bytes()).await?;

    // Stream incoming messages
    let mut stream = client.conversations().stream_all_messages().await?;
    while let Some(message) = stream.next().await {
        println!("New message: {}", String::from_utf8_lossy(&message.content));
    }

    Ok(())
}
```

## What You Can Build

**💬 Wallet Chat Apps** - Build the next generation of messaging apps where your wallet is your identity

**🤖 Token-Gated Bots** - Create bots that only respond to holders of specific NFTs or tokens

**📱 dApp Notifications** - Send transactional messages directly to user wallets

**🎮 Gaming Communication** - Enable player-to-player messaging in blockchain games

**🏛️ DAO Coordination** - Build governance tools with encrypted member communication

**💼 DeFi Alerts** - Send real-time updates about positions, liquidations, or opportunities

## Core Features

### 🔑 **Identity & Authentication**
- **Wallet-based identity** - Your Ethereum address is your username
- **Signature-based auth** - Prove ownership without revealing private keys
- **Passkey integration** - Seamless authentication with WebAuthn coming soon
- **Cross-chain support** - Works with Ethereum, Polygon, and other EVM chains

### 💬 **Messaging Primitives**
- **1:1 conversations** - Direct encrypted messaging between two addresses
- **Group chats** - Secure group conversations with access control
- **Message attachments** - Send files, images, and rich media
- **Message reactions** - React to messages with emojis and custom reactions

### 🔒 **Security & Privacy**
- **MLS encryption** - Built on the IETF Messaging Layer Security standard
- **Metadata protection** - Message timing and patterns are obscured
- **Local key management** - Keys never leave your device

### 🌐 **Network & Infrastructure**
- **Decentralization roadmap** - Moving from federated to fully decentralized architecture
- **Offline support** - Queue messages when offline, sync when reconnected
- **Message persistence** - Reliable delivery with automatic retries
- **Efficient sync** - Only download messages you haven't seen
- **Configurable storage** - SQLite, PostgreSQL, or custom backends

## Platform Support

| Platform | Status | Language | Package |
|----------|--------|----------|---------|
| **iOS** | ✅ Production | Swift | [`xmtp-ios`](https://github.com/xmtp/xmtp-ios) |
| **Android** | ✅ Production | Kotlin | [`xmtp-android`](https://github.com/xmtp/xmtp-android) |
| **React Native** | ✅ Production | JavaScript | [`@xmtp/react-native-sdk`](https://www.npmjs.com/package/@xmtp/react-native-sdk) |
| **Web/Node.js** | ✅ Production | JavaScript | [`@xmtp/mls-client`](https://www.npmjs.com/package/@xmtp/mls-client) |
| **WebAssembly** | 🚧 Beta | WASM | [`libxmtp-wasm`](https://github.com/xmtp/libxmtp) |

## Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────────────────┐
│   Your App      │    │    libxmtp      │    │      XMTP Network           │
│                 │    │                 │    │                             │
│  ┌───────────┐  │    │  ┌───────────┐  │    │  ┌─────┐  ┌─────┐  ┌─────┐  │
│  │    UI     │◄─┼────┼─►│  Client   │◄─┼────┼─►│Node │◄─┤Node │◄─┤Node │  │
│  └───────────┘  │    │  └───────────┘  │    │  └─────┘  └─────┘  └─────┘  │
│                 │    │        │        │    │     │        │        │     │
│                 │    │  ┌───────────┐  │    │  ┌─────┐  ┌─────┐  ┌─────┐  │
│                 │    │  │   Store   │  │    │  │Node │  │Node │  │Node │  │
│                 │    │  └───────────┘  │    │  └─────┘  └─────┘  └─────┘  │
└─────────────────┘    └─────────────────┘    └─────────────────────────────┘
```

## Examples

### Token-Gated Bot
```rust
// Only respond to messages from NFT holders
if client.verify_nft_ownership(sender_address, nft_contract).await? {
    conversation.send("Welcome, NFT holder! 🎨".as_bytes()).await?;
}
```

### DeFi Notifications
```rust
// Send liquidation warning
let conversation = client.conversations()
    .create_dm(user_wallet)
    .await?;

conversation.send(format!(
    "⚠️ Your position is at risk! Current ratio: {:.2}%",
    collateral_ratio
).as_bytes()).await?;
```

### Group Chat with Admins
```rust
let group = client.conversations()
    .create_group_with_permissions(
        members,
        GroupPermissions::AdminOnly
    ).await?;
```

## Contributing

We're actively looking for contributors! Check out our [Contributing Guide](CONTRIBUTING.md) and [Good First Issues](https://github.com/xmtp/libxmtp/labels/good%20first%20issue).

**Areas where we need help:**
- 🔧 Protocol optimizations and performance improvements
- 🌐 Additional language bindings (Python, Go, C++)
- 📱 Mobile-specific optimizations
- 🧪 Testing infrastructure and edge case coverage
- 📚 Documentation and example applications

## Resources

- **📖 [Developer Docs](https://xmtp.org/docs/)** - Complete integration guides
- **🎮 [Quickstart Tutorial](https://xmtp.org/docs/tutorials/quickstart)** - Build your first XMTP app in 10 minutes
- **💬 [Discord Community](https://discord.gg/xmtp)** - Get help from the team and community
- **🐦 [Twitter](https://twitter.com/xmtp_)** - Follow for updates and announcements
- **🔧 [Example Apps](https://github.com/xmtp/example-apps)** - Reference implementations

## License

**MIT** - Build anything, commercial or open source.

---

**Ready to build the future of communication?**

Star this repo ⭐ and [join our Discord](https://discord.gg/xmtp) to connect with other builders pushing the boundaries of decentralized messaging.

*Made with ❤️ by the XMTP team and contributors worldwide*

## Requirements

- Install [Rustup](https://rustup.rs/)
- Install [Docker](https://www.docker.com/get-started/)
- Install
  [Foundry](https://book.getfoundry.sh/getting-started/installation#using-foundryup)

## Development

Start Docker Desktop.

- To install other dependencies and start background services:

  ```bash
  dev/up
  ```

  Specifically, this command creates and runs an XMTP node in Docker Desktop.

- To run tests:

  ```bash
  RUST_LOG=off cargo test
  ```

  Many team members also install and use `cargo nextest` for better test
  isolation and log output behavior.

- run tests and open coverage in a browser:

```bash
./dev/test/coverage
```

- To run WebAssembly tests headless:

  ```bash
  dev/test/wasm
  ```

- To run WebAssembly tests interactively for a package, for example, `xmtp_mls`:

  ```bash
  dev/test/wasm-interactive xmtp_mls
  ```

- To run browser SDK tests:

  ```bash
  dev/test/browser-sdk
  ```

## Tips & Tricks

### Log Output Flags for Tests

- Output test logs in a async-aware context-specific tree format with the
  environment variable `CONTEXTUAL`

```
CONTEXTUAL=1 cargo test
```

- Filter tests logs by Crate

```
RUST_LOG=xmtp_mls=debug,xmtp_api=off,xmtp_id=info cargo test
```

- Output test logs as in a structured JSON format for inspection with
  third-party viewer

```
STRUCTURED=1 cargo test
```

- Two ways to replace InboxIds/InstallationIds/EthAddresses with a
  human-readable string name in logs

_NOTE_: Only works when using `CONTEXTUAL=1` flag. So to get the replacement,
`CONTEXTUAL=1 cargo test`

1.)

Before the test runs, add an `TestLogReplace` declaration to the top
`replace.add` accepts two arguments: the string to replace in logs and the
string to replace it with. Note that on dropping the "TestLogReplace" object,
the replacements will no longer be made.

```rust
let mut replace = TestLogReplace::default();
replace.add(alix.installation_id(), "alix_installation_id");
```

2.) Build the `TesterBuilder` `with_name`

```rust
let tester = Tester::builder().with_name("alix").build().await;
```

This replaces all instances of alix's InboxIds, InstallationIds and Identifiers
with "alix", "alix_installation", "alix_identifier" respectively, in test output
logs.

## Quick Start (Dev Containers)

This project supports containerized development. From Visual Studio Code Dev
Containers extension specify the Dockerfile as the target:

`Reopen in Container`

or

Command line build using docker

```bash
docker build . -t libxmtp:1
```

## Quick Start (nix)

This project has an option to use nix as the development environment. Nix sets
up a reproducible & deterministic environment of the dependency tree libxmtp
requires. In the future the hope is to cover all SDKs -- currently, Android &
Wasm are best supported. Flake outputs are cached with
[determinate nix](https://docs.determinate.systems/). Determinate is a
distribution of nix catered towards developers & CI with sophisticated caching
ability.

### Install

use the `./dev/nix-up` script and follow the prompts. this will install
determinate nix & direnv. Direnv is a useful tool to auto-load default nix
environments (with your consent, given `direnv allow` && `direnv deny` commands)
with your already-used shell environment.

### Uninstall

use the `./dev/nix-down` script & follow prompts. this will uninstall nix &
direnv.

### Using direnv

to configure direnv for a project, run the command
`echo "use flake" . > .envrc"` in the project root. direnv will prompt you to
allow the environment which can be done with `direnv allow`. using a non-default
environment (ex: android) can be done using `nix develop .#environment`. EX:
`nix develop .#android`. the environment description must be available in nix
flake `devShells` output.

## Structure

libxmtp/

├ [`bindings_ffi`](./bindings_ffi): FFI bindings for Android and iOS (in
progress)

├ [`bindings_wasm`](./bindings_wasm): Wasm bindings (in progress)

├ examples/

│ ├ [`android/xmtpv3_example`](./examples/android/xmtpv3_example): Example
Android app (in progress)

│ └ [`cli`](./examples/cli): Example XMTP console client. Use the CLI to try out
sending double ratchet messages on the XMTP `dev` network.

├ [`xmtp_api_grpc`](./xmtp_api_grpc): API client for XMTP's gRPC API, using code
from `xmtp_proto`

├ [`xmtp_cryptography`](./xmtp_cryptography): Cryptographic operations

├ [`xmtp_mls`](./xmtp_mls): Version 3 of XMTP which implements
[Messaging Layer Security](https://messaginglayersecurity.rocks/).

├ [`xmtp_proto`](./xmtp_proto): Generated code for handling XMTP protocol
buffers

### Run the benchmarks

**possible benchmarks include:**

- `group_limit`: benchmarks surrounding maximum members adding/removed from
  group
- `crypto`: benchmarks surrounding cryptographic functions

**Example Commands**

- **Run a specific category of benchmark**
  `cargo bench --features bench -p xmtp_mls --bench group_limit`
- **Run against dev grpc** DEV_GRPC=1 cargo bench --features bench -p xmtp_mls
  --bench group_limit
- **Just run all benchmarks** ./dev/bench
- **Run one specific benchmark** ./dev/bench add_1_member_to_group
- **Generate flamegraph from one benchmark** ./dev/flamegraph
  add_1_member_to_group

## Code Coverage

Code coverage is generated using `cargo llvm-cov` and is integrated into ci and
reported to [codecov](https://codecov.io).

To run the tests locally you can run the `dev/llvm-cov` script to run the same
workspace tests and generate both an lcov and html report.

If you have installed the `Coverage Gutters` extension in vscode (or a
derivative) you can get coverage information in your IDE.

## Contributing

See our [contribution guide](./CONTRIBUTING.md) to learn more about contributing
to this project.
