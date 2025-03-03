# XMTP MLS

This document describes how XMTP implements [Messaging Layer Security](https://messaginglayersecurity.rocks/) (MLS).
:::

## Crate `xmtp-mls`

The `xmtp-mls` crate contains the core of XMTP's MLS implementation. It uses [OpenMLS](https://github.com/openmls/openmls) to implement the MLS protocol.

## Clients

An XMTP MLS client has the following responsibilities

1. Manage connections to the network through an API client
2. Manage the state of its local SQLite database
3. Provide an `MlsProvider` to the OpenMLS library for all cryptographic operations
4. Authenticate messages and identities

These primitives are then used to construct and modify groups, send messages, read and authenticate messages from other users, and modify a user's inbox state.

Each client instance and SQLite database is bound to a single Inbox ID and a single Installation ID. If a developer wishes to operate using a different Inbox ID or Installation ID, they must create a new client with a new database.

### Initialization

When initializing a new client instance with a fresh database the SDK will generate a new identity private key and store it in its local SQLite database for future use. Subsequent instantiations of the client will use the keys stored in the database.
This means that XMTP does not use unique keys for each group but reuses the signature key for all groups of the client.

The client must then attach its installation keys to an XMTP Inbox before it can create or be added to groups. The process for this depends on the state of the inbox at the time of client creation.

1. There is no inbox registered with the network for the wallet address/nonce combination specified as part of client setup.
2. There is an inbox registered with the network for the wallet address/nonce combination specified, but the inbox does not yet have the installation keys registered.
3. The inbox is registered with the network and the client's installation keys are associated with the inbox.

In the case of (1), the client will generate an identity update that includes the `CreateInbox` and `AddAssociation` actions, sign it with the installation keys, and then expects the application to gather the required wallet signature before proceeding. Once the required wallet signature has been provided the client will upload a key package signed by the installation key and the signed identity update to the server. If both requests are accepted, the client is ready to send and receive messages and join groups. If the nonce is `0`, and the application has a set of XMTP V2 keys available, we allow `CreateInbox` actions to be signed with the V2 keys instead of the wallet. Each set of V2 keys is signed by a wallet, so we treat the V2 key as a proxy for the wallet. This allows for a one-time migration of existing XMTP users to the new inbox system.

For (2), the client will generate an `AddAssociation` identity update linking the installation keys with the inbox. The client will sign the update with its installation keys. The application is expected to gather a wallet signature from any wallet already linked to the inbox and attach that signature to the identity update as well. Once the signature has been collected, the client will upload a signed key package and the signed identity update to the server. V2 keys may only be used to sign this update if the `nonce` for the inbox is `0` and the V2 keys have not been used to sign any previous identity updates.

In the case of (3), the client is available for immediate use and the application does not need to provide any additional signatures or upload a new key package.

### Create Group

Any client may create a new group and can control the initial [permission policies](#permissions-and-metadata) applied to that group.

Groups are initialized with one member (the creator's installation), and the `GroupMembership` extension will have the creator's `inbox_id` mapped to a `sequence_id` of 0. The first time any action is performed on the group (sending a message, adding members, updating metadata, etc) the client is expected to create a commit that updates the creator's `sequence_id` to the current value in the `GroupMembership` mapping. This will automatically add the creator's other installations to the group if any exist.

The creator of the group will always be specified as the only member of the `super_admin_list` in the group's metadata at the time of creation.

Any valid `PolicySet` can be specified as part of group creation. A `PolicySet` is considered valid if it can be serialized according to the Protocol Buffer type.

### Sync Welcomes

Each client is able to use the `QueryWelcomes` API to get a list of their welcome messages. This API is paginated using a cursor. The client persists its last seen cursor in its local database so that any call to `sync_welcomes` will only return unseen results.

For each welcome found in the sync, the client will attempt the Join by Invite flow specified below.

### Join by Invite

Each Welcome message contains two layers of encryption. The outer payload is encrypted with HPKE, using the `hpke_init_key` from the recipient's key package. The plaintext of this decrypted payload is a TLS serialized standard MLS welcome message.

The client must validate the MLS welcome message according to the spec, and additionally ensure that the members of the MLS group match the expected list of installations according to the `GroupMembership` mapping at the specified `sequence_id` for each member.
The Welcome contains the MLS ratchet tree of the group such that no additional queries are required.

### Key Rotation

Group members are expected to periodically update their path encryption secret. This happens:

1. Before sending their first message to the group
2. Before sending a message, if 3 months have elapsed since their last path update.

Signature keys are not rotated, as their public key forms a persistent identity for the account.

### Revocation

Installations and wallets may be revoked by publishing a `RevokeAssociation` IdentityUpdate as specified in [XIP-46](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-46-multi-wallet-identity.md).

Revoking an installation does not immediately remove it from groups that it is already a member of. Instead, any group member is allowed to update the group membership to point to the latest `sequence_id` for that member, which will remove the installation from the group. Clients will periodically check for updates to group member installations as part of their `sync` process, so this typically happens quickly, but the protocol makes no guarantees about the timeliness of group member removals following revocation.

XMTP does not detect or remove inactive clients. Inactive clients need to be detected by the application, which should then trigger a removal of the client.

## Identity

When initializing a client with a new database, we need to link the randomly generated installation keys with an XMTP inbox. We do this by creating a specific string that describes the association (as described in XIP-46) and signing it. The signature needs to be recoverable to an address that is already linked to the Inbox ID.

For `CreateInbox` actions, this must be an address where `SHA256(CONCAT(wallet_address, nonce)) == inbox_id`. For `AddAssociation` actions, the recovered address must be an existing member of the inbox that has not been revoked.

### From EOA Wallet Signature

The signer of an Externally Owned Account (EOA) wallet can be verified and recovered by recovering the address using the ECDSA signature and the expected signature text. The recovered address must match the expected address, since an ECDSA signature will recover to a random address if the `recover_signer` function is executed against different text than what was actually signed.

### From a Smart Contract Wallet Signature

Smart Contract Wallets are a cryptocurrency wallets where the signature can only be verified by calling a specific smart contract on an EVM compatible blockchain according to the [ERC-1271](https://eips.ethereum.org/EIPS/eip-1271) specification. Smart Wallet signatures are not recoverable and the ERC-1271 specification does not make any assumptions about the format of the signature or the type of verification performed in the smart contract.

The Smart Contract used to validate the signature is mutable, so a signature that was valid at one point in time may be invalid at an earlier or later time. In order to make our signature validation deterministic, we store a block number alongside the signature so that other users may verify the signature at the point in time at which it was originally signed. If the smart contract changes later in a way that invalidates the signature, we consider that out of scope of our security model and do not need to invalidate the association. Associations remain valid until they are revoked.

Client applications must include RPC URLs for all XMTP supported blockchains as part of client instantiation in order to validate smart wallet signatures. The signature is validated using the [EIP-6492](https://eips.ethereum.org/EIPS/eip-6492) standard and a "universal validator" smart contract, which allows for the verification of ERC-1271 signatures from Smart Contract Wallets that have not yet been deployed.

### From V2 Identity

XMTP V2 is a legacy protocol which used a different identity model. In order to migrate users from V2 -> MLS, we allow signatures using their legacy keys in a narrow set of circumstances.

**Legacy V2 keys may only be used to create one association (globally)**

We enforce this in two ways. Legacy V2 keys may only be used on an Inbox ID with nonce 0. Replay protection prevents the same Legacy V2 key from being used multiple times on that inbox ID.

We chose these restrictions because the legacy identity model shares keys between multiple apps and devices. If those keys were compromised, any associated installations would also need to be treated as compromised. We did not want to allow users to have all of their installation keys associated with one set of legacy keys, since that would mean that all installations would need to be revoked if any installations needed to be revoked.

A legacy keypair includes a signature to link the XMTP public key with a given wallet. The challenge for the signature is in the following format:

```
XMTP : Create Identity
$SERIALIZED_V2_PUBLIC_KEY

For more info: https://xmtp.org/signatures/
```

To validate a V2 identity signature you must first recover the wallet address from the signature on the public key, then verify that the signature on the association challenge recovers to the same public key. If the chain of signatures validates correctly you can treat the association as if it was signed by the wallet.

### Uniqueness

The system must protect against re-use of signatures, since that could be used to compromise the protocol. Signature re-use between different inboxes is protected against by including the Inbox ID in all challenge text. Within the same inbox, the raw signature must be stored and clients must check that any new identity action does not re-use previously seen identity actions.

## Validation

There are two levels of validation for MLS based applications.
The validation that the MLS protocol performs, here implemented with OpenMLS, and the validation required by the application for the MLS protocol to provide its guarantees.

### Commits

In addition to all validations performed by OpenMLS, `libxmtp` is expected to perform the following additional validations on commit messages.

1. Ensure the commit is allowed according to the permissions policies on the group (see below).
2. Validate the credentials and key packages of any new members to the group according to the guides below.
3. Ensure that the actual change in MLS group members matches the expected change in membership found by diffing the previous `GroupMembership` struct and the new `GroupMembership`.

There is currently no mechanism to detect, report, or recover from group splits due to invalid commits.
This may have to be solved by the application rather than the SDK.

### Key Packages

New clients are expected to upload a Key Package to the network signed by their installation public key. This Key Package is visible to all other users on the network and used to encrypt welcome messages to invite the installation to conversations.

When validating another user's Key Package, clients must perform all the standard MLS validations (signature and message authenticity checks), and additionally validate that the installation key is associated with the `inbox_id` referenced in the Key Package's credential.

This validation is performed by downloading the latest identity updates for the `inbox_id` and ensuring that the installation key is present in the list of associated keys.

Clients are expected to regularly rotate their key package to limit the impact if the HPKE keypair referenced in the key package is compromised. This rotation is expected to happen any time the client receives a new welcome message. Clients may batch this rotation, so that if they receive N welcome messages at once they only have to rotate one time. Clients are expected to keep at most 2 HPKE keypairs (one from the current Key Package and one from the previous Key Package).

### Credentials

The MLS credential used in Key Packages and leaf nodes contains a single field: `inbox_id`. Clients are expected to validate this credential by resolving the state of that `inbox_id` (as described in XIP-46
) and ensuring that the installation key that has signed the credential is a current member of the inbox.

XMTP currently does not implement credential rotation because they are long-lived connection between the MLS client and the `inbox_id`.

## Group Policies

Each group is configured with a set of policies that control which users are allowed to perform certain restricted actions.

- `add_member_policy`: Add new Inboxes to the group
- `remove_member_policy`: Remove Inboxes from the group
- `update_metadata_policy`: A mapping containing policies for each metadata field
- `add_admin_policy`: Designate the "admin" role to a member of the group
- `remove_admin_policy`: Remove the "admin" designation for a group member
- `update_permissions_policy`: Update the set of policies for a group.

By default, each group has the creator's Inbox ID specified as a "super admin".

These policies are stored in a MLS GroupContextExtension represented by the `GroupMutablePermissions` struct.

### Where are these enforced?

Policies are enforced as part of commit validation. All restricted actions are communicated through MLS commit messages. Any commit that makes changes to the group state and violates the policies specified on the group must be rejected wholly (a commit that includes valid and invalid changes must be completely rejected).

### Permissions and Metadata

XMTP uses the following GroupContextExtensions in each MLS group.

1. `GroupMembers`: Stores the list of `inbox_id`s and the current `sequence_id` for each inbox. Governed by the `add_member_policy` and `remove_member_policy`.
2. `GroupMutableMetadata`: User-defined metadata for the group. Changes are governed by the `update_metadata_policy` and changes to the list of admins contained in this metadata is governed by the `add_admin_policy` and `remove_admin_policy`.
3. `GroupMutablePermissions`: Store the current permissions policies for the group. Changes are governed by the `update_permissions_policy`

## Storage

### Sqlite backend

The XMTP SQLite database is optionally encrypted using [SQLCipher](https://www.zetetic.net/sqlcipher/). App developers are strongly encouraged to use an encryption key for the database, and to store that encryption key securely, but the derivation, storage, and encryption of this key is considered out of scope for the protocol and is the responsibility of the app.

The database is used for both, the MLS state, as well as the decrypted messages.

## Crate `xmtp-id`

- Implements XIP 43
- Handles signature validation
- Provides identity management for XMTP inboxes
- Implements wallet address verification for both EOA and Smart Contract Wallets
- Supports legacy V2 identity migration
- Defines core types like `InboxId` and `InboxIdRef`
- Provides the `InboxOwner` trait for wallet interactions
- Includes utilities for smart contract detection and verification

## MLS

In this section we describe the parameters, selected for instating MLS, as well as map the [MLS architecture](https://datatracker.ietf.org/doc/html/draft-ietf-mls-architecture) to the deployed system.

### Parameter Selection

The MLS group is [built](https://github.com/xmtp/libxmtp/blob/428826ecc7b86ac49787db4c9a49eb0e63e7a05e/xmtp_mls/src/groups/mod.rs#L1218-L1225) with the following parameters.

- ciphersuite: MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519
- maximum past epochs: 3
- maximum forward ratchets: 1000 (OpenMLS default)

The number of maximum past epochs limits the amount of key material that is kept for past epochs.
It is a trade-off between functionality and forward secrecy and should only be enabled if the Delivery Service cannot guarantee that application messages will be sent in the same epoch in which they were generated.
The value of 3 here is lower than the default value of 5 in OpenMLS and thus increases security over the default setting.

The number of maximum forward ratchet is defined by OpenMLS' default of 1000.
This defines how many ratchets into the future OpenMLS does in order to try finding the correct key to decrypt a message.

The cryptographic primitives are defined in the ciphersuite:
x25519 is used for key exchange,
Chach20Poly1305 as AEAD for symmetric encryption,
Sha2-256 for hashing, and
Ed25519 for signatures.

XMTP uses the [MLS secret tree](https://www.rfc-editor.org/rfc/rfc9420.html#name-secret-tree) and AEAD for encrypting application messages.

### Authentication Service (AS)

The purpose of the AS is to link the public key in the leaf nodes to a user.
The tasks of the AS are defined in [RFC 9420 Section 5.3.1](https://www.rfc-editor.org/rfc/rfc9420.html#section-5.3.1).
In particular does the AS ensure that presented identifiers in the credential are correctly associated with the `signature_key` field in the leaf node.

XMTP does not implement a separate AS service, but implements a direct binding of the XMTP Inbox ID with the public signature key of the member's leaf nodes.
See [XIP 46](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-46-multi-wallet-identity.md) for details.

### Delivery Service (DS)

The DS describes the set of mechanisms used to transport messages and key packages between clients. XMTP uses a central server that chat messages and invite/Welcome messages can be submitted to and read from without special permission. However, it is per-IP rate-limited in order in order to mitigate spam.

The server provides APIs for submitting and reading (MLS-encrypted) chat messages by their group ID and Welcome messages by the installation key. Reading a message from the server does not chage the server state. The order in which the server receives the messages determines the order in which they are returned to clients.

Clients can query group invites (in the form of Welcome messages) by their installation key. The Welcome messages have an additional layer of encryption using HPKE, applied at the client side. This is done because when creating a commit with several Add proposals, a large part of the Welcome messages sent to each user is shared, which would make it possible to see that several users have been added to the same group. The additional layer of encryption makes these messages unlinkable.

It is possible that a client fails to decrypt an incoming messages. Possible sources of decryption failures could be that an attacker mounts a DoS attack by sending spam, or that the group state of clients somehow diverged, such that honest clients use different encryption keys and thus produce undecryptable messages. This should not be possible and would require a bug in the XMTP SDK or OpenMLS, and that bug to be tripped inadvertently or adversarially.

Should decryption of an incoming message fail, In the SDK, messages that fail to decrypt are just dropped. The XMTP messaging app uses telemetry to notify the developers of decryption failures, so they have some understanding of whether this problem needs additional mitigation. Should the data indicate that this is in fact a real problem, methods for group state fork discovery and recovery mechanisms will be developed and deployed.

## Security Considerations

This section defines the threat model XMTP has for libxmtp.

**Out of scope** are the following areas

- Physical attacks on endpoints
- Network privacy (IP addresses)
- Deniability of messages
- Membership privacy (inherited from MLS)
- Attacks based on the ciphertext size (XMTP does not use extra padding)

### Security properties

The security of XMTP rests on that of MLS. The chosen configuration of MLS provides privacy and authenticity of all messages. It also provides both Forward Secrecy and Post-Compromise Security. Since no ciphersuite has been standardized that provides protection against Quantum Adversaries (even HNDL attacks), XMTP also does not provide security against such attackers.

#### Endpoint security

In the XMTP messaging app, private key material is stored in the secure key storage of the mobile operating systems. XMTP encourages app developers using the SDK to do the same, but ultimately it is their decision.

### Credential Validation

Identities, linked to credentials, rely on Ethereum wallet addresses.
XMTP does not expect any validation beyond that.
This can be seen as a type of key transparency mechanism.

### Denial of Service and Spam Protection

XMTP implements IP based rate limiting for all API endpoints for DoS and spam protection.
No other authentication is required.

### Privacy

XMTP offers push notifications with FCM. Push token privacy is not in scope for XMTP's threat model.

But XMTP adds additional anonymity to Welcome messages by encrypting them with HPKE to hide their metadata. Further, only private messages that leak the minimum amount of metadata are used.

### Consent

XMTP allows blocking invites to groups if unwanted, based on the inviter.

### Trust in Backend

The backend maintains a directory that maps Wallet Addresses to Inbox IDs and may return wrong information. Backends are able to hide revocations of bindings.

### Transport Channels

The [MLS architecture](https://datatracker.ietf.org/doc/html/draft-ietf-mls-architecture) recommends in Section 8.1. to use transport channels that are reliable and hide metadata.
This is because MLS messages may still leak metadata.
However, note that [XMTP uses private messages everywhere](https://github.com/xmtp/libxmtp/blob/3af97cb69435e5b9daa4577bad6a3bd187834d97/xmtp_mls/src/groups/mod.rs#L1222C29-L1222C45), which has the highest metadata hiding properties possible in MLS.
Further note that the secure transport is supposed to protect MLS and XMTP metadata but is not necessary for the end-to-end security of the MLS-based messaging.

To hide the remaining metadata, XMTP uses GRPC with TLS.
TLS is instantiated through [Rustls](https://github.com/xmtp/libxmtp/blob/3af97cb69435e5b9daa4577bad6a3bd187834d97/xmtp_api_grpc/src/grpc_api_helper.rs#L59).

XMTP uses the default TLS configuration provided by Rustls via the `ClientTlsConfig::new().with_enabled_roots()` method. This configuration uses the system's root certificate store and the default set of modern and secure ciphersuites supported by Rustls, which typically includes TLS 1.2 and TLS 1.3 with AEAD ciphers like AES-GCM and ChaCha20-Poly1305.

### Last Resort Key Packages Only

[Section 10 of the MLS RFC](https://www.rfc-editor.org/rfc/rfc9420.html#section-10) states that key packages are intended to be used only once and SHOULD NOT be reused.
This is to ensure that the keys used to encrypt welcome messages are ephemeral, i.e. used only once.
[Section 16.8](https://www.rfc-editor.org/rfc/rfc9420.html#name-keypackage-reuse) gives more detail on the security impact of reusing key packages.

While there have been [discussions](https://mailarchive.ietf.org/arch/msg/mls/0a-Q30vGLla4eFmJNPGWW8W5baE/) on the exact security impact of re-using key packages, it remains an open question how an attack on re-used key packages would look like.

Reusing a key package is equivalent to using the same static key to encrypt towards multiple times.
While this is not a security issue in itself, it allows a potential attacker to collect multiple ciphertexts for the same public key, which combined with other factors like weak randomness, can lead to serious attacks.

Due to the decentralized nature of the XMTP protocol, it is almost impossible to use ephemeral key packages.
Instead, XMTP implements a protocol to ensure that key packages are rotated as soon after use as possible.
In particular does a client change its key package after receiving a welcome message that used the published key package.
