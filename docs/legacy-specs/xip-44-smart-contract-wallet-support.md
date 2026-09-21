---
xip: 44
title: Smart Contract Wallet Support
description: Explores a way for developers building with XMTP to allow their users to sign up and log in to XMTP inboxes using Smart Contract Wallets
author: Richard Hua (@richardhuaaa), Nicholas Molnar (@neekolas), Andrew Plaza (@insipx), Cameron Voell (@cameronvoell), Tong Wang (@37ng)
status: Final
type: Standards
category: Interface
created: 2024-03-25
implementation: https://docs.xmtp.org/chat-apps/core-messaging/create-a-signer#create-a-smart-contract-wallet-signer
---

## Abstract

XMTP currently only allows new user sign-ups and XMTP inbox login via EVM standard [Externally Owned Accounts](https://ethereum.org/en/developers/docs/accounts/) (EOAs). This XIP explores one way to update XMTP SDKs and XMTP node software so that developers integrating XMTP can allow their users to sign up and log in to their XMTP inboxes using Smart Contract Wallets going forward.

## Motivation

As of March 2024, Smart Contract Wallet adoption is seen by many in the web3 space as a key driver of improvements in user experience and increased security (see [How Smart accounts and account abstraction can unlock Ethereum’s full utility, Cointelegraph 3/21/24](https://cointelegraph.com/news/ethereum-smart-accounts-utility)).

Key benefits of Smart Contract Wallets over EOA accounts include:

1. Increased security via customizable signature verification functions

2. Improved UX via newly supported signing methods ([passkeys](https://github.com/passkeys-4337/smart-wallet)), configurable account recovery [options](https://www.gate.io/learn/articles/what-is-a-social-recovery-wallet/676), and more [options](https://medium.com/coinmonks/account-abstraction-a-tool-against-gas-fees-in-your-dapp-322d66670395) for paying for transactions

## Specification

### Credential Updates

Credentials are a key primitive in XMTP used for linking user signing keys with MLS installation keys. See the [MLS Spec](https://www.rfc-editor.org/rfc/rfc9420.html#name-credentials).

Today in XMTP, credentials only support [ERC-191](https://eips.ethereum.org/EIPS/eip-191) signatures.

With the following updates, we can support both ERC-191 and [ERC-1271](https://eips.ethereum.org/EIPS/eip-1271) signature formats. See [github.com/xmtp/proto](https://github.com/xmtp/proto/blob/main/proto/mls/message_contents/association.proto).

```protobuf
// RecoverableEcdsaSignature
message RecoverableEcdsaSignature {
  // 65-bytes [ R || S || V ], with recovery id as the last byte
  bytes bytes = 1;
}

message Erc1271Signature {
  // CAIP-10 contract address
  // https://github.com/ChainAgnostic/CAIPs/blob/main/CAIPs/caip-10.md
  string contract_address = 1;
  // Specify the block height to verify the signature against
  int64 block_height = 2;
  // The actual signature bytes
  bytes signature = 3;
}

message LegacyCreateIdentityAssociation {
  // Signs SHA-256 hash of installation key
  RecoverableEcdsaSignature signature = 1;
  // created_ns is encoded inside serialized key, account_address is recoverable
  // from the SignedPublicKey signature
  xmtp.message_contents.SignedPublicKey signed_legacy_create_identity_key = 2;
}

message Association {
  oneof kind {
    :RecoverableEcdsaSignature erc_191 = 1;
    Erc1271Signature erc_1271 = 2;
    LegacyCreateIdentityAssociation legacy_xmtp_identity = 3;
  }
}

// Used for "Grant Messaging Access" associations
message GrantMessagingAccessAssociation {
  AssociationTextVersion association_text_version = 1;
  Association association = 2;
  uint64 created_ns = 3;
}
```

A `block_height` is associated with ERC-1271 signatures at the time of XMTP node initial verification, so that future verifiers can verify the signature based on the blockchain state at the time the signature was granted instead of the current state. This means that a previously valid signature should not become invalid later.

### Block Height Validation

In the event that an ERC-1271 signer for a given contract address is removed or replaced, it must not be possible for a malicious third party with possession of the signer keys to retroactively create a `GrantMessagingAccessAssociation` referencing an earlier `block_height` in which the signer was valid.

To prevent this, the nodes must honor a `block_tolerance` for each blockchain, which is a number of blocks roughly corresponding to 30 minutes of time. Whenever a new credential is registered, each node MUST verify that the `block_height` on the signature is no more than `block_tolerance` behind the current `block_height` as observed by the node, and discard the credential if it fails this test. This ensures that sufficient time is allocated for node replication, while bounding the time window in which a compromised signer can be used retroactively. In the event that nodes disagree on whether a given credential is valid, the credential has an undefined state on the network.

### Credential Verification

With the addition of ERC-1271 signatures, credential verification can no longer happen offline. In order to verify the signature, the verifier will need to make an RPC call to the appropriate blockchain. We will also need to maintain an allowlist of chains that XMTP supports Smart Contract Wallets on.

We would introduce a new crate into `libxmtp` named `credential_verifier` that exposes the following interface:

```rust
pub enum AssociationType {
    EOA,
    SmartContract,
    Legacy
}

pub struct VerifiedCredential {
    pub account_address: String,
    pub association_type: AssociationType,
}

pub struct VerificationRequest {
    installation_pub_key: Vec<u8>,
    credential: Vec<u8>,
}

type VerificationResult = Result<VerifiedCredential, VerificationError>;

#[async_trait]
pub trait CredentialVerifier {
    async fn verify_credential(request: VerificationRequest) -> VerificationResult;
    async fn batch_verify_credentials(
        credentials_to_verify: Vec<VerificationRequest>,
    ) -> Result<Vec<VerificationResult>, VerificationError>;
}
```

This library will be used inside the `mls_validation_service` and can be accessed directly from inside LibXMTP. Wrapper implementations of this interface are possible, which might delegate the verification to remote services or add a layer of caching on results.

We propose that we include an API for remotely accessing this service as part of our Node’s MLS API that allows for batch operations. This API would proxy calls to the `mls_validation_service` internally and include caching. This remote service exists primarily for performance reasons and is not a launch requirement. Client applications can opt-in to use this remote service once available. In a world where XMTP has multiple node operators, clients may also choose to query multiple remote verification services to ensure that no service is acting maliciously, since a remote verification service would require trust from the client.

Remote verification services have the advantage of centralizing access and authorization for all the blockchain networks required to verify credentials. We can put all charges on our Infura/Alchemy bill, instead of forcing developers to each configure RPC provider access for every client.

### Account discovery + XMTP ID

After enabling 1271-signatures, we will enable creation of accounts controlled by Smart Contract Wallets that may not have an EOA associated with them. The account in this case should be represented by the chain ID and Smart Contract Address of the Smart Contract Wallet that created the account (see [CAIP-10](https://github.com/ChainAgnostic/CAIPs/blob/main/CAIPs/caip-10.md)). To add someone with this type of account to a group or conversation, we would need to call `add_member` with the CAIP-10 chain-specified contract address.

## Rationale

The above proposal aims to get Smart Contract Wallet support in XMTP with minimal migration overhead, and without introducing new sign-up fees, while also minimizing developer integration complexity.

### Alternatives Considered

If XMTP Protocol switched to an onchain identity system, managed by smart contracts, installation keys signed by a valid signer key could be stored onchain at the time of initial verification. Future verification could just check that the installation keys are stored in a smart contract, without needing to check an ERC-1271 signature against an earlier `block_height`. The main barriers to this solution were the introduction of transaction costs for new user sign-ups and the ability to support identities cross-chain. Though these transaction costs could be subsidized by someone other than XMTP end users, it was not clear how to keep that type of subsidization for new user costs sustainable and decentralized at this point in time.

## Test Cases

The first class of test cases would be to ensure that Smart Contract Wallet users could do all actions in XMTP that EOA users can do today.

The second main class of tests would be to ensure that cases around Smart Contract Wallet signing method updates would behave as intended:

1. If a user with valid installation keys rotates their signing method for their Smart Contract Wallet, installation key credentials inspected by others should still pass ERC-1271 signature validation.
2. If an XMTP user with a Smart Contract Wallet has a signing method that is compromised, and then rotates away the compromised signing method, the compromised signing method should not be able to sign XMTP credentials that pass validation for more than the designated `block_tolerance` as described in the **Specification** section above.

## Backward Compatibility

None. Given the alpha state of groups, we can force developers to wipe their local databases and (if needed) wipe the dev database as well.

### Migration

SCW accounts would only be possible once we have deprecated V2 and migrated 1:1 messaging to use MLS. Until then, SCW accounts would only be able to use XMTP via Groups.

As part of the MLS migration, there would be a time when messages would be “dual-sent” to both V2 and V3 networks. During this time we could begin onboarding smart contract-only wallets.

## Security Considerations

Remote verification services move trust to the server providing the verifications. Clients can audit and confirm the results are accurate if desired. Once we have multiple node operators, clients can also query against multiple verification services. This may still have performance benefits relative to performing all queries locally since many verifications can be wrapped in a single HTTP/gRPC request and the remote service can safely cache verification results.
