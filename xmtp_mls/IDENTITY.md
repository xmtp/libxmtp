# XMTP Identity Structure

In XMTP v3, a messaging account is represented by an Ethereum wallet address. An account consists of multiple app installations that may send and receive messages on behalf of it. Each installation is a separate cryptographic identity with its own set of keys.

```
Amal's account (Ethereum wallet address)
│
├── Converse app (mobile phone)
│   └── Installation key bundle 1
│
├── Coinbase Wallet (mobile phone)
│   └── Installation key bundle 2
│
├── Lenster (tablet)
│   └── Installation key bundle 3
│
└── Coinbase Wallet (tablet)
    └── Installation key bundle 4
```

Using per-installation keys provides the following benefits:

- Installation private keys are never shared across devices or published onto the network.
- The user may enumerate the installations that have messaging access to their account.
- The user may revoke keys on a per-installation level.

**Installation provisioning**

Every new app installation gains messaging access as follows:

1. A new Ed25519 signature key pair is generated and stored on the device, representing the installation's identity.
2. The app prompts the user to sign the public key with their Ethereum wallet, establishing an association between the installation's identity and the user’s account. Example text:

   ```
   XMTP: Grant Messaging Access

   Current Time: <current time and local timezone>
   Installation Key: <hex(last_20_bytes(keccak256(Ed25519PublicKey)))>
   ```

3. The following data is protobuf-serialized to form the MLS Credential. The credential can be presented alongside the installation key to prove an association with an account:

   ```
   MlsCredential {
       Eip191Association {
           association_text_version: i32,
           signature: bytes,
           wallet_address: string,
           creation_time_ns: i64,
       }
   }
   ```

4. A last resort KeyPackage (signed by the installation key per the MLS spec) is generated and stored on the device.
5. The app publishes the public signing key, credential, and last resort key package to the server, which stores it under the account. Other apps may query for this information to understand that the installation is on the network, associated with the account, and how to contact it.

**Installation management**

At any time, the user may enumerate active installations by querying for all identity updates under the account. The user may identify each installation by the creation time as well as the installation key from the signing text.

In the event of a compromise, malicious app, or no longer used installation, the user may revoke an installation’s messaging access going forward by signing a revocation payload containing the installation’s identity keys using their wallet and publishing it to the server. This will subsequently be surfaced in the identity update list for clients to validate.

**Authentication service**

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

How are key packages exchanged and how does this affect forward secrecy/PCS with ephemeral keys if there are multiple devices with the same inbox keys?

Registering a new installation:

1. Generate and store identity key and credential as above.
2. Group ID of `account:<wallet_address>`
   External join (or create if does not exist)
   - Other members of the group will validate that this join is from the same wallet address, and ignore the commit otherwise.
3. [Export a group secret](https://openmls.tech/openmls/doc/openmls/group/struct.MlsGroup.html#method.export_secret) and use it to derive a key pair - we can call this the 'Inbox' key pair. Save the inbox key pair on-device.
4. Sign the inbox key pair using the identity key.
5. Upload the identity key, credential, and inbox key to the server.

Revoking an installation:

Updating existing groups:

Could be done by auto-refresh
Or could be done by new installation - sync group conversations list (mechanism is), then do external commit to replace the inbox key pair
Other installations of group need to verify that old and new inbox key pairs are signed by the same wallet, and the new inbox key pair is the latest for the given user.

Sending a message:

Sign the message using

Other installations:
