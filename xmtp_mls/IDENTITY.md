# XMTP Identity Structure

In XMTP v3, a messaging account is represented by an Ethereum wallet address. An account consists of multiple app installations that may send and receive messages on behalf of it. Each installation is a separate cryptographic identity with its own set of keys.

```
Amal's account (Ethereum wallet address)
│
├── Converse app (mobile phone)
│   └── Installation key bundle 1
│
├── Coinbase Wallet app (mobile phone)
│   └── Installation key bundle 2
│
├── Lenster app (tablet)
│   └── Installation key bundle 3
│
└── Coinbase Wallet app (tablet)
    └── Installation key bundle 4
```

Using per-installation keys provides the following benefits:

- Installation private keys are never shared across devices or published onto the network.
- The user may enumerate the installations that have messaging access to their account.
- The user may revoke keys on a per-installation level.

## Ethereum wallet

Today, an Ethereum wallet consists of a secp256k1 keypair, and is identified by a a public address, which is the hex-encoding of the last 20 bytes of the Keccak-256 hash of the public key, prepended by `0x`. The user is expected to have a pre-existing Ethereum wallet prior to onboarding with XMTP.

The wallet keys can be used to sign arbitrary text, with most wallet software requiring explicit [user acceptance](https://docs.metamask.io/wallet/how-to/sign-data/#use-personal_sign) of the signature text. The signature text is formatted according to version `0x45` of [EIP-191](https://eips.ethereum.org/EIPS/eip-191), and is signed via a recoverable ECDSA signature.

Wallet signature requests originating from XMTP will additionally prepend context to the EIP-191 `message` field to prevent collisions between signatures in different contexts:

```
XMTP: <Label>\n\n
```

| Label                   | Described in section                                    |
| ----------------------- | ------------------------------------------------------- |
| Grant messaging access  | [Installation provisioning](#installation-provisioning) |
| Revoke messaging access | [Installation revocation](#installation-revocation)     |

There is currently no way to rotate the keys associated with an Ethereum wallet. In the event that a wallet is compromised, the user must create a new wallet.

## Installation provisioning

XMTP installations hold a long-lived Ed25519 key-pair (the 'installation key') and are identified via the Ethereum addressing format. The public installation key is used as the `signature_key` in all MLS leaf nodes, and is associated with the account's wallet via a wallet-signed credential. Every new app installation gains messaging access as follows:

1. The new Ed25519 key pair (installation key) is generated and stored on the device.
2. The app prompts the user to sign the public key with their Ethereum wallet. The user is expected to inspect the text and reject the signing request if the data is invalid, for example if the displayed time is incorrect. Association text version 1 format:

   ```
   XMTP: Grant Messaging Access

   Current Time: <ISO 8601 date and time with local UTC offset>
   Installation ID: <hex(last_20_bytes(keccak256(Ed25519PublicKey)))>
   ```

3. The signature and related data is then protobuf-serialized to form the MLS Credential:

   ```
   struct {
        association_text_version: i32,
        signature: bytes,
        creation_iso8601_time: string,
        wallet_address: string,
   } Eip191Association;


   struct {
       installation_public_key: bytes,
       eip191_association: Eip191Association
   } MlsCredential;
   ```

4. A last resort KeyPackage (containing the credential and signed by the installation key per the [MLS spec](https://www.rfc-editor.org/rfc/rfc9420.html#name-key-packages)) is generated and stored on the device.
5. The app publishes the last resort key package to the server. The server will provide all valid last resort key packages for a given wallet address to any client that requests it.

## Credential validation

## Installation revocation

At any time, the user may enumerate active installations by querying for all identity updates under the account. The user may identify each installation by the creation time as well as the installation key from the signing text.

In the event of a compromise, malicious app, or no longer used installation, the user may revoke an installation’s messaging access going forward by signing a revocation payload containing the installation’s identity keys using their wallet and publishing it to the server. This will subsequently be surfaced in the identity update list for clients to validate.

## Authentication service

Unlike in other messaging providers, XMTP accounts are key-pairs. This means that the link between an installation and an account can be achieved via a signature that can be programmatically validated on any client device. There is no need for a centralized service to validate credentials, nor for safety numbers to be shown in apps built on top of XMTP. Additionally, apps built on top of XMTP may choose to layer on decentralized name resolution protocols such as ENS in order to display a user-friendly name.

Although registrations and revocations cannot be forged, we currently rely on trust in centralized XMTP servers not to maliciously hide installation registration and revocation payloads from clients that request them. Current decentralization efforts within XMTP will eventually produce a trustless public immutable record of registrations and revocations that does not rely on any single entity.

## Synchronizing MLS group membership

At the user level, messages are exchanged between accounts, however at the cryptographic level, messages are exchanged between installations. This poses the question of how MLS group membership can be kept up-to-date as installations are registered and revoked and members are added and removed. This requires two components which will be addressed in reverse order:

1. How to know which accounts are members of a group
2. How to know which installations belong to each account

**Mapping from accounts to installations**

The latter can be addressed using the mechanism described in the earlier section - any participant in a conversation can query for updates on any account in the conversation and construct the current list of valid installations. If the current installation list does not match what is in the group, the participant may add or remove installations in the group to match the latest state, and all other participants may perform the same verification. This can be performed at any frequency (for example before every message send), with various performance optimizations possible.

**Mapping from conversations to accounts**

Will add this in next PR.
