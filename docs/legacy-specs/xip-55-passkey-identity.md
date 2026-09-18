---
xip: 55
title: Passkey identity
description: Proposes a flexible identity model where new identity types can be adopted without needing coordination between all clients.
authors: Dakota Brink (@codabrink), Nick Molnar (@neekolas), Naomi Plasterer (@nplasterer)
discussions-to: https://community.xmtp.org/t/xip-55-passkey-identity/874
status: Draft
type: Standards
category: XRC
created: 2025-02-12
---

## Abstract

[XIP-46](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-46-multi-wallet-identity.md) defined a flexible identity model that allows for XMTP to aggregate many accounts and devices into a single XMTP identity, but the only supported account type so far is Ethereum.

Adding an individual new account type, such as Bitcoin or Solana, would require coordination between all clients before the new account type could be adopted. The coordination cost of this type of change has prevented XMTP from adopting additional types of accounts.

This proposed change allows for an even more flexible identity model where new identity types can be adopted without needing coordination between all clients.

## Motivation

XMTP uses signed Identity Updates to solve two problems:

1. Associate many installations (app-specific keys) with an inbox on the XMTP network, allowing many apps to send and receive messages from one XMTP identity
2. Identify an inbox by an external account (such as an Ethereum wallet) allowing users to find an inbox by wallet address.

`1` provides a critical security property: By looking at the chain of Identity Updates for an inbox, anyone on the network can tell if a given installation is an authorized member of the inbox. This needs to happen consistently between every client on the network to guarantee the authenticity of messages.

The goal of this XIP is to introduce flexibility in `2` while keeping `1` simple enough that it can be adopted consistently by 100% of the apps on the network.

## Specification

### New account type

This XIP introduces a new type of account to the protocol: A `secp256r1` key. These are the types of keys used in Passkeys, which have wide support across operating systems and built-in mechanisms for synchronization across devices.

`secp256r1` keys would be allowed to be used in any of the ways that Ethereum accounts can be used today:

- As the recovery address for an inbox
- As an input to derive the `inbox_id`
- As a member of the inbox, which can then authorize installations

All clients would need to upgrade to a version of the XMTP SDK that supports `secp256r1` keys as a first-class citizen.

### Additional association types

With Passkeys as a first-class citizen, additional association types can start to be added to the protocol without requiring full support across all clients.

Passkeys would be used as the mechanism to link many installations together, while these new association types would be used to identify inboxes on the network and let others look up users by external account addresses.

These new association types would always be leaves in the identity tree, and not able to authorize new installations underneath them. When a client reads an Identity Update that it doesn't understand, it would skip that update and move on to the next. This allows the introduction of something like a Bitcoin Wallet association type and safely deploy it to clients without needing 100% of clients to have upgraded to a version that supports Bitcoin associations.

These new association types would be searchable through an API, so that any client can look up an inbox by any associated identifier.

### Rollout

To ensure a smooth and safe rollout to apps, passkey identity will be implemented in two phases:

1. Introduce `secp256r1` accounts and get to 100% adoption across the network
2. Begin rolling out additional association types

### App layer changes

With this change, apps need to expect that an inbox may be associated with 0-N Ethereum addresses, and any number of additional association types. This is technically true today, since an inbox may change its recovery address and then revoke any associated accounts. However, it may be safe to assume that most developers aren't accounting for this edge case and assume there is at least 1 Ethereum account associated with an inbox.

Associating a new account type would require a signature from an existing member of the inbox (either installation or account) and a signature from the new associated account.

As additional account types are introduced to the protocol apps will want to expose those identities to the user. In some cases, they may want to resolve account addresses to names using naming protocols native to the chain (for example, looking up Solana accounts with SNS).

Developers will need to choose which name to display to other users when multiple names are present. This is true today, as an Ethereum account can be associated with many ENS names, but this change may increase the urgency for protocol-level support for a user-specified name prioritization scheme.

### Other considerations

Here are some "gotchas" around XMTP's other signature types that need to be considered.

#### Relying party identifiers

Unlike XMTP's other signature types, the signatures of passkeys are bound to relying party identifiers to protect against phishing. The signing provider generally gets to dictate what this value is, so the signer will have to provide it along with the public key for XMTP to track.

#### Installation IDs

XMTP address verification in the inbox ID generation algorithm will need to be tweaked to account for the new p256 address type.

## Rationale and alternatives

### Why does XMTP need Passkeys?

To authenticate messages and group membership, XMTP needs all clients to have a consistent way of validating that the installation that signed the message is a member of an inbox. This is how XMTP prevents impersonation on the network. If this were implemented without passkeys, and each new association was made equivalent to EVM addresses today, each additional account type would be a backward-incompatible change that would need to be coordinated across the network. Passkeys allow XMTP to only add one new top-level account used for authenticating messages.

Passkeys were specifically introduced to allow for inboxes that don't have any Ethereum account associated. If the goal were to only allow for additional association types *on top* of a required Ethereum address, the "Additional association types" part of this proposal could be implemented without the "New account type" section.

## Backward compatibility

### Compatibility with old versions of LibXMTP

#### Phase 1

Adding support with passkeys in Phase 1 will break compatibility with old versions of LibXMTP because passkeys will have the ability to sign identity updates with the p256 curve. LibXMTP must also be made to gracefully handle the `MemberIdentifier` proto potentially coming in as `None` as new `MemberIdentifiers` are introduced in preparation for Phase 2.

#### Phase 2

Phase 2 will add a Solana `MemberIdentifier`, and will *not* break compatibility with Phase 1 or newer LibXMTP releases due to new `MemberIdentifiers` not being able to sign identity updates, and the graceful handling of new unknown `MemberIdentifiers`.

### Inbox state

Inbox state has both a `recovery_address: String` field and an `addresses: Vec<String>` field. Because XMTP will now be supporting more chains than just Ethereum, this needs to change. First, to draw attention to the change, the fields will be renamed to `recovery_identifier` and `identifiers`, and the `String` type will be changed to a new struct: `PublicIdentifier` which is basically a more public-facing version of `MemberIdentifier`. The updated `InboxState` will look like this:

```rust
#[napi(object)]
pub struct InboxState { 
  pub inbox_id: String,
  pub recovery_identifier: PublicIdentifier,
  pub installations: Vec<Installation>,
  pub identifiers: Vec<PublicIdentifier>,
}

#[napi(object)]
pub enum PublicIdentifier {
  Ethereum(String),
  Passkey(Vec<u8>),
}

impl PublicIdentifier {
  fn display(&self) -> String {
    // ...
  }
}
```

## Reference implementation

### High-level technical implementation

#### Phase 1: Adding passkey support (breaking)

- Add a new `MemberIdentifier::Passkey` variant
  - The `Passkey` variant needs to record the relying party identifier along with the public key.
  - Rename `MemberIdentifier::Address` to `MemberIdentifier::EthAddress` to distinguish from the new `MemberIdentifier` variants coming in
- Add a new `UnverifiedSignature::Passkey` variant
  - Implement the associated `to_verified` branch logic
- Add a new `SignatureKind::P256` variant
- Rename `associate_wallet` to `associate_member` and change it to take in a `MemberIdentifier` enum instead of a `new_wallet_address`, and block on the `MemberIdentifier::Installation` variant.
- Adapt [generate_inbox_id](https://github.com/xmtp/libxmtp/blob/71b47a20020ecece139cdc0d56ab356d3f6637f1/xmtp_id/src/associations/hashes.rs#L22) to work with passkey addresses, mainly updating the valid address check.
- Create a `PublicIdentifier` type, which is a public-facing version of `MemberIdentifier` and doesn’t contain potentially sensitive metadata like the passkey’s relying partner.
- Rename `InboxState.account_addresses` to `InboxState.identifiers` and change the type to `Vec<Identifier>`
  - Also rename `InboxState.recovery_address` to `recovery_identifier` and change the type to `Identifier`
- Filter out unknown `MemberIdentifiers` that may come from new associations.

#### Phase 2: Adding Solana support (non-breaking)

- Add a new `MemberIdentifier::Solana` variant that has restricted permissions. It will not be able to perform any identity updates, nor will it be allowed as the recovery identity.
- XMTP will **not** be adding Solana variants for `UnverifiedSignature` or `SignatureKind`

This means that the `MemberIdentifier` in the `AddAssociation` proto may come through as `None` in old versions that haven't received the Solana update. It must be verified that this behavior is acceptable.

## Security considerations

### Threat model

#### Insecure association types

Because these new association types can be looked up through the API, any flaws in the implementation of signature verification could allow one user to impersonate another. New association types must be vetted with the same level of security review as any other protocol change.

## Copyright

Copyright and related rights waived via [CC0](https://creativecommons.org/publicdomain/zero/1.0/).
