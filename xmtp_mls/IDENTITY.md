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

## Identity lifecycle

### Ethereum wallet

As of Nov 30 2023, an Ethereum wallet consists of a secp256k1 keypair, and is identified by a public address, which is the hex-encoding of the last 20 bytes of the Keccak-256 hash of the public key, prepended by `0x`. The wallet keys do not expire and are not rotatable - in the event of a compromise, the user must create a new wallet. The user is expected to have a pre-existing Ethereum wallet prior to onboarding with XMTP.

The wallet keys can be used to sign arbitrary text, with most wallet software requiring explicit [user acceptance](https://docs.metamask.io/wallet/how-to/sign-data/#use-personal_sign) of the signature text. The signature text is formatted according to version `0x45` of [EIP-191](https://eips.ethereum.org/EIPS/eip-191), and is signed via a recoverable ECDSA signature.

Wallet signature requests originating from XMTP will additionally prepend context to the EIP-191 `message` field to prevent collisions between signatures in different contexts:

```
XMTP: <Label>\n\n
```

| Label                   | Described in section                                    |
| ----------------------- | ------------------------------------------------------- |
| Grant messaging access  | [Installation registration](#installation-provisioning) |
| Revoke messaging access | [Installation revocation](#installation-revocation)     |

### Installation registration

XMTP installations consist of a long-lived Ed25519 key-pair (the 'installation key') and are identified via the Ethereum addressing format. The public installation key is used as the `signature_key` in all MLS leaf nodes, and is associated with the account's wallet via a wallet-signed credential. Every new app installation gains messaging access as follows:

1. The new Ed25519 key pair (installation key) is generated and stored on the device.
2. The app prompts the user to sign the public key with their Ethereum wallet. The user is expected to inspect the text and reject the signing request if the data is invalid, for example if the displayed time is incorrect. The format for version 1 of the association text is as follows:

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
        iso8601_time: string,
        wallet_address: string,
   } Eip191Association;


   struct {
       installation_public_key: bytes,
       eip191_association: Eip191Association
   } MlsCredential;
   ```

4. A last resort KeyPackage (containing the credential and signed by the installation key per the [MLS spec](https://www.rfc-editor.org/rfc/rfc9420.html#name-key-packages)) is generated and stored on the device.
5. The app publishes the last resort key package to the server. The server will provide all identity updates (registrations and revocations) for a given wallet address to any client that requests it.

### Credential validation

In XMTP there is no need for a centralized authentication service to validate credentials, nor for safety numbers to be shown in apps built on top of XMTP, as clients can locally validate that a credential is valid.

Credential validation must be performed by clients at the [events described by the MLS spec](https://www.rfc-editor.org/rfc/rfc9420.html#name-credential-validation) as follows:

1. Verify that the referenced `installation_public_key` matches the `signature_key` of the leaf node.
1. Derive the association text using the `association_text_version`, `creation_iso8601_time`, `installation_public_key`, with a label of `Grant Messaging Access`.
1. Recover the wallet public key from the recoverable ECDSA `signature` on the association text.
1. Derive the wallet address from the public key and verify that it matches the `wallet_address` on the association.

### Installation revocation

_Note: Revocation is not scheduled to be built until Q2 2024 or later_

Users may revoke an installation as follows:

1. Enumerate active installations by querying for all identity updates under the account. The user may identify each installation by the creation time as well as the installation public key of the credential.
1. Select the installation to revoke.
1. The app prompts the user to sign the revocation with their Ethereum wallet. The user is expected to inspect the text and reject the signing request if the data is invalid, for example if the displayed time is incorrect. The format for version 1 of the association text is as follows:

   ```
   XMTP: Revoke Messaging Access

   Current Time: <ISO 8601 date and time with local UTC offset>
   Installation ID: <hex(last_20_bytes(keccak256(Ed25519PublicKey)))>
   ```

1. The signature and related data is then protobuf-serialized to form the revocation:

   ```
   struct {
       installation_public_key: bytes,
       eip191_association: Eip191Association
   } InstallationRevocation;
   ```

1. The app publishes the revocation to the server. The server will provide all identity updates (registrations and revocations) for a given wallet address to any client that requests it.

Validation of revocations is identical to the process described in [Credential Validation](#credential-validation), except that a label of `Revoke Messaging Access` is used.

Once an installation is revoked, it cannot be re-registered or re-provisioned. The time displayed on the revocation is for informational purposes only.

### Installation synchronization

At any time in the course of a conversation, the list of valid installations for the participating wallet addresses may change via registration or revocation.

Clients must perform the following validation prior to publishing each payload on the group:

1. Assemble a list of wallet addresses in the conversation from the leaf nodes.
1. Fetch all identity updates on those wallet addresses, and construct a list of valid installations.
1. Remove nodes from the conversation that are not in the list of valid installations.
1. Add nodes to the conversation that are in the list of valid installations and not already present.

Clients must perform the following validation on receiving each payload in the group:

1. Query the server for identity updates on the sending wallet address and verify that the sending installation has not been revoked.

XMTP clients may perform performance optimizations, such as caching installation lists with a short TTL.

An open area of investigation is ensuring transcript consistency in the face of revoked clients. If the server can maintain a strict ordering between revocations and payloads in the conversations (such as commits performed by the revoked clients), then all participants may apply the updates in a consistent way.

### Delivery service and server trust

Although registrations and revocations cannot be forged, we currently rely on trust in centralized XMTP servers not to maliciously hide installation registration and revocation payloads from clients that request them. Current decentralization efforts within XMTP will eventually produce a trustless public immutable record of registrations and revocations that does not rely on any single entity.

## Synchronizing MLS group membership

At the user level, messages are exchanged between accounts, however at the cryptographic level, messages are exchanged between installations. This poses the question of how MLS group membership can be kept up-to-date as installations are registered and revoked and members are added and removed. This requires two components which will be addressed in reverse order:

1. How to know which accounts are members of a group
2. How to know which installations belong to each account

**Mapping from accounts to installations**

The latter can be addressed using the mechanism described in the earlier section - any participant in a conversation can query for updates on any account in the conversation and construct the current list of valid installations. If the current installation list does not match what is in the group, the participant may add or remove installations in the group to match the latest state, and all other participants may perform the same verification. This can be performed at any frequency (for example before every message send), with various performance optimizations possible.

**Mapping from conversations to accounts**

Will add this in next PR.

Clients must perform the following validation when a member is added:

Clients must perform the following validation when a member is removed:
