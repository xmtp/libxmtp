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

As of Nov 30 2023, an Ethereum wallet consists of a secp256k1 keypair, and is identified by a public address, which is the hex-encoding of the last 20 bytes of the Keccak-256 hash of the public key, prepended by `0x`. Wallet keys do not expire and are not rotatable - in the event of a compromise, the user must create a new wallet. The user is expected to have a pre-existing Ethereum wallet prior to onboarding with XMTP.

The wallet keys can be used to sign arbitrary text, with most wallet software requiring explicit [user acceptance](https://docs.metamask.io/wallet/how-to/sign-data/#use-personal_sign) of the signature text. The signature text is formatted according to version `0x45` of [EIP-191](https://eips.ethereum.org/EIPS/eip-191), and is signed via a recoverable ECDSA signature.

Wallet signature requests originating from XMTP will additionally prepend context to the EIP-191 `message` field to prevent collisions between signatures in different contexts:

```
XMTP: <Label>\n\n
```

| Label                   | Described in section                                    |
| ----------------------- | ------------------------------------------------------- |
| Grant messaging access  | [Installation registration](#installation-registration) |
| Revoke messaging access | [Installation revocation](#installation-revocation)     |

### Installation registration

XMTP installations consist of a long-lived Ed25519 key-pair (the 'installation key') and are identified via the Ethereum addressing format. The public installation key is used as the `signature_key` in all MLS leaf nodes and nowhere else, and is associated with the account's wallet via a wallet-signed credential. Every new app installation gains messaging access as follows:

1. The new Ed25519 key pair (installation key) is generated and stored on the device.
2. The app prompts the user to sign the public key with their Ethereum wallet. The user is expected to inspect the text and reject the signing request if the data is invalid, for example if the account address is not the one they intended. The format for version 1 of the association text is as follows:

   ```
   XMTP : Grant Messaging Access

   Current Time: <ISO 8601 date and time in UTC>
   Account Address: <ethereum address>
   Installation ID: <hex(last_20_bytes(keccak256(Ed25519PublicKey)))>

   For more info: https://xmtp.org/signatures/
   ```

3. The signature and related data is then protobuf-serialized to form the MLS Credential:

   ```
   struct {
        association_text_version: i32,
        signature: bytes,
        iso8601_time: string,
        account_address: string,
   } Eip191Association;


   struct {
       installation_public_key: bytes,
       eip191_association: Eip191Association
   } MlsCredential;
   ```

4. A last resort KeyPackage (containing the credential and signed by the installation key per the [MLS spec](https://www.rfc-editor.org/rfc/rfc9420.html#name-key-packages)) is generated and stored on the device.
5. The app publishes the last resort key package to the server, which implicitly serves as a registration. The server will provide all identity updates (registrations and revocations) for a given wallet address to any client that requests it.

### Credential validation

Apps built on XMTP have a reduced need for safety numbers to be shown, as clients can locally validate that a credential is valid.

Credential validation must be performed by clients at the [events described by the MLS spec](https://www.rfc-editor.org/rfc/rfc9420.html#name-credential-validation) as follows:

1. Verify that the referenced `installation_public_key` matches the `signature_key` of the leaf node.
1. Derive the association text using the `association_text_version`, `creation_iso8601_time`, `installation_public_key`, with a label of `Grant Messaging Access`.
1. Recover the wallet public key from the recoverable ECDSA `signature` on the association text.
1. Derive the wallet address from the public key and verify that it matches the `account_address` on the association.

### Installation revocation

_Note: Revocation is not scheduled to be built until Q2 2024 or later_

Users may revoke an installation as follows:

1. Enumerate active installations by querying for all identity updates under the account. The user may identify each installation by the creation time as well as the installation public key of the credential.
1. Select the installation to revoke.
1. The app prompts the user to sign the revocation with their Ethereum wallet. The user is expected to inspect the text and reject the signing request if the data is invalid, for example if the account address is not the one they intended. The format for version 1 of the association text is as follows:

   ```
   XMTP : Revoke Messaging Access

   Current Time: <ISO 8601 date and time in UTC>
   Account Address: <ethereum address>
   Installation ID: <hex(last_20_bytes(keccak256(Ed25519PublicKey)))>

   For more info: https://xmtp.org/signatures/
   ```

1. The signature and related data is then protobuf-serialized to form the revocation:

   ```
   struct {
       installation_public_key: bytes,
       eip191_association: Eip191Association
   } InstallationRevocation;
   ```

1. The app publishes the revocation to the server. The server will provide all identity updates (registrations and revocations) for a given wallet address to any client that requests it.
1. The installation performing the revocation enumerates all known groups that it is a member of, and submits proposals to remove the revoked installation. Additional removals may also occur via the process described in [Installation synchronization](#installation-synchronization).

Validation of revocation payloads is identical to the process described in [Credential Validation](#credential-validation), except that a label of `Revoke Messaging Access` is used.

Once an installation is revoked, it cannot be re-registered or re-provisioned. The time displayed on the revocation is for informational purposes only.

Revocations may not apply immediately on all groups. In order to ensure transcript consistency, payloads from revoked installations are considered valid if they were published before the installation was removed from the group.

### Installation synchronization

At any time in the course of a conversation, the list of valid installations for the participating wallet addresses may change via registration or revocation.

Clients must perform the following validation prior to publishing each payload on the group, as well as periodically. XMTP clients may perform performance optimizations, such as caching installation lists with a short TTL.

1. Assemble a list of wallet addresses in the conversation from the leaf nodes.
1. Fetch all identity updates on those wallet addresses.
1. Validate the credentials and revocations and construct a list of valid installations (registered installations minus revoked installations).
1. Publish a commit to remove nodes from the conversation that are not in the list of valid installations.
1. Publish a commit to add nodes to the conversation that are in the list of valid installations and not already present.

These commits must include an attached proof (credential or revocation). When validating add/remove commits, clients must verify either that the proposer has permissions to add/remove accounts from the group, or that a proof of installation revocation was attached to the commit.

### Server trust

We currently rely on trust in centralized XMTP servers, which could do the following if malicious.

1. **Omit payloads**. A malicious server could hide registrations (and application messages) from valid installations in order to censor them, or revocations for invalid installations in order to prevent post-compromise recovery.
   - Decentralization efforts within XMTP aim to produce an immutable log of identity updates (H1 2024), with immutability of conversation payloads coming later.
1. **Reorder payloads**. A malicious server could reorder messages and commits within a conversation.
   - Decentralization efforts within XMTP aim to produce a consensus-driven ordering of commits at minimum (H2+ 2024).

Note, however, that a malicious server is unable to forge registration and revocation payloads without access to the wallet keys used to sign them.

## Account-level membership

At the user level, messages are exchanged between accounts, however at the cryptographic level, messages are exchanged between installations. Because of this two-layer system, there must exist a mechanism for mapping between accounts and installations in a conversation.

We use an implicit mapping - an account is considered a participant of a conversation if one or more installations from the account are present in the tree.

- **Listing accounts**. To list accounts, we enumerate unique wallet addresses from the credentials of the leaf nodes in the conversation.
- **Adding accounts**. To add an account, publish a commit adding one or more installations belonging to the account to the conversation. Note that if we fail to include all valid installations belonging to the account, it will be resolved via the process described in [Installation Synchronization](#installation-synchronization).
- **Removing accounts**. To remove an account, publish a commit removing _all_ installations belonging to the account from the conversation. It is important that all installations belonging to the account that are present in the tree during the epoch in which the commit is published are removed, and we ensure this is the case by enforcing linear ordering of epochs.

### Other approaches

Account/user trees are another approach for solving this problem, however we assume the complexity of managing key packages and welcome messages in this system is higher than the current system, and have therefore deferred this. We are open to feedback otherwise!
