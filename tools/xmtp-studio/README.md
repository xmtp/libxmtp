# 💬 LibXMTP Studio & MLS Encrypted Messaging

An interactive **Messaging Layer Security (MLS v3) Encryption Visualizer**, **Epoch Key Ratchet Monitor**, and **XIP-46 Multi-Wallet Identity Console** for **LibXMTP (`xmtp/libxmtp`)**.

---

## 🌟 Key Features

- 💬 **Messaging Layer Security (MLS / RFC 9420)**: E2E encrypted group messaging with HPKE forward secrecy and epoch key rotation.
- 🔐 **XIP-46 Multi-Wallet Key Association**: Register installation keypairs linked to EVM wallet identities.
- 🌐 **Interactive Web Studio**: Real-time encrypted messaging stream and key inspector on `http://localhost:3428`.
- ⌨️ **Universal CLI (`xmtp-cli`)**: Terminal utility for sending encrypted messages and managing identities.

---

## 🚀 Quickstart

```bash
# Launch LibXMTP Studio
npm start
# Open http://localhost:3428

# Or run via CLI
node bin/xmtp-cli.js send "Hello XMTP MLS Agent"
node bin/xmtp-cli.js identity "0xUserWalletAddress"
```
