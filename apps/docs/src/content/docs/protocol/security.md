---
title: Messaging security
---

XMTP delivers end-to-end encrypted 1:1 and group chat using the following resources:

- Advanced cryptographic techniques
- Secure key management practices
- MLS ([Messaging Layer Security](https://www.rfc-editor.org/rfc/rfc9420.html))

Specifically, XMTP messaging provides the comprehensive security properties covered in the following sections. In these sections, **group** refers to the MLS concept of a group, which includes both 1:1 and group conversations.

🎥 **walkthrough: XMTP and MLS**

This video provides a walkthrough of XMTP's implementation of MLS.

<iframe
  width="560"
  height="315"
  src="https://www.youtube.com/embed/g6I9qXOkDMo?si=o5pD2xwa_yynoP5s"
  title="YouTube video player"
  frameborder="0"
  allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
  referrerpolicy="strict-origin-when-cross-origin"
  allowfullscreen
></iframe>

To dive deeper into how XMTP implements MLS, see the [XMTP MLS protocol specification](https://github.com/xmtp/libxmtp/tree/main/crates/xmtp_mls).

## Security properties

### Message confidentiality

Ensures that the contents of messages in transit can't be read without the corresponding encryption keys.

Message confidentiality is achieved through symmetric encryption, ensuring that only intended recipients can read the message content. [AEAD](#cryptographic-tools-in-use) (Authenticated Encryption with Associated Data) is used to encrypt the message content, providing robust protection against unauthorized access.

### Forward secrecy

Ensures that even if current session keys are compromised, past messages remain secure.

MLS achieves this by using the ratcheting mechanism, where the keys used to encrypt application messages are ratcheted forward every time a message is sent. When the old key is deleted, old messages can't be decrypted, even if the newer keys are known. This property is supported by using ephemeral keys during the key encapsulation process.

### Post-compromise security

Ensures that future messages remain secure even if current encryption keys are compromised.

XMTP uses regular key rotation achieved through a commit mechanism with a specific update path in MLS, meaning a new group secret is encrypted to all other members. This essentially resets the key and an attacker with the old state can't derive the new secret, as long as the private key from the leaf node in the ratchet tree construction hasn't been compromised.

### Message authentication

Validates the identity of the participants in the conversation, preventing impersonation.

XMTP uses digital signatures to strongly guarantee message authenticity. These signatures ensure that each message is cryptographically signed by the sender, verifying the sender's identity without revealing it to unauthorized parties. This prevents attackers from impersonating conversation participants.

### Message integrity

Ensures that messages can't be tampered with during transit and that messages are genuine and unaltered.

XMTP achieves this through the use of MLS. The combination of digital signatures and [AEAD](#cryptographic-tools-in-use) enables XMTP to detect changes to message content.

### Quantum resistance

Protects against future quantum computer attacks through post-quantum cryptography.

XMTP implements quantum-resistant encryption to protect against "Harvest Now, Decrypt Later" (HNDL) attacks, where adversaries store encrypted messages until quantum computers become powerful enough to break current encryption. XMTP uses a hybrid approach that combines post-quantum algorithms with conventional cryptography, ensuring protection against future quantum threats without compromising current security.

The quantum resistance is implemented by securing Welcome messages (the entry point for all conversations) with post-quantum key encapsulation. Since Welcome messages contain group secrets, this protects those secrets against quantum attacks on the Welcome key encapsulation. It does not make every part of the protocol quantum-resistant. Once inside a group, all messages maintain the same size and performance characteristics as before.

To learn more about how XMTP achieves quantum resistance, see [XMTP and the Future of Privacy in a Quantum World](https://community.xmtp.org/t/xmtp-and-the-future-of-privacy-in-a-quantum-world/1079).

### User anonymity

Ensures that outsiders can't deduce the participants of a group, users who have interacted with each other, or the sender or recipient of individual messages.

User anonymity is achieved through a combination of the following functions:

- MLS Welcome messages encrypt the sender metadata and group ID, protecting the social graph.

- XMTP adds a layer of encryption to MLS Welcome messages using [HPKE](#cryptographic-tools-in-use) (Hybrid Public Key Encryption). This prevents multiple recipients of the same Welcome message from being correlated to the same group.

- XMTP uses MLS [PrivateMessage](https://www.rfc-editor.org/rfc/rfc9420.html#name-confidentiality-of-sender-d) framing to hide the sender and content of group messages.

- The backend treats envelope payloads as opaque bytes. It does not verify MLS group membership or MLS signatures. Clients must perform these checks. Only legitimate members possess the correct encryption keys for a given group.

It's technically possible for backend operators to analyze query patterns per IP address. However, clients may choose to obfuscate this information using proxying/onion routing.

XMTP currently hides the sender of Welcome messages (used to add users to a group) but doesn't hide the Welcome message recipients. This makes it possible to determine how many groups a user was invited to but not whether the user did anything about the invitations.

## Trust boundary

The backend operator can observe topic identifiers, request timing, request size, and client connection metadata. The operator cannot decrypt valid MLS message content without a group key. Clients receive unsigned backend envelopes over a trusted transport and then validate the enclosed protocol data. See the [message security specification](/specs/003-message-security/) for the full trust boundary.

## Cryptographic tools in use

XMTP messaging uses the ciphersuite _MLS_128_HPKEX25519_CHACHA20POLY1305_SHA256_Ed25519_.

Here is a summary of individual cryptographic tools used to collectively ensure that XMTP messaging is secure, authenticated, and tamper-proof:

- [HPKE](https://www.rfc-editor.org/rfc/rfc9180.html)

  Used to encrypt Welcome messages, protect the identities of group invitees, and maintain the confidentiality of group membership. We use the ciphersuite HPKEX25519.

- [AEAD](https://developers.google.com/tink/aead)

  Used to ensure both confidentiality and integrity of messages. In particular, we use the ciphersuite CHACHA20POLY1305.

- [SHA3_256 and SHA2_256](http://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf)

  XMTP uses two cryptographic hash functions to ensure data integrity and provide strong cryptographic binding. SHA3_256 is used in the multi-wallet identity structure. SHA2_256 is used in MLS. The ciphersuite is SHA256.

- [Ed25519](https://ed25519.cr.yp.to/ed25519-20110926.pdf)

  Used for digital signatures to provide secure, high-performance signing and verification of messages. The ciphersuite is Ed25519.

- [XWING KEM](https://www.ietf.org/archive/id/draft-connolly-cfrg-xwing-kem-02.html)

  Used for quantum-resistant key encapsulation in Welcome messages. XWING is a hybrid post-quantum KEM that combines conventional cryptography with [ML-KEM](https://csrc.nist.gov/pubs/fips/203/final) (the NIST-standardized post-quantum component), providing protection against future quantum computer attacks while maintaining current security standards.

## FAQ about messaging security

1. **Can XMTP read user messages?**

   No, messages are encrypted end-to-end. Only participants in a conversation have the keys to decrypt the messages in it. A participant's app decrypts messages locally.

2. **How does XMTP's encryption compare to Signal or WhatsApp?**

   XMTP provides the same security properties (forward secrecy and post-compromise security) as Signal and WhatsApp, using the newer, more efficient MLS protocol.

3. **Can others see who users are messaging with?**

   The backend cannot read encrypted message content or directly identify group members from that content. It can observe topic identifiers, timing, sizes, and connection metadata. Query patterns can reveal relationships.

4. **What happens if a user loses access to their wallet?**

   An existing installation can retain access through its local keys and database. A linked identity can authenticate a new installation. If the user loses all identities and installation keys, they cannot recover that inbox. See [inboxes and installations](/sdk/inboxes/).

5. **Are group messages as secure as direct messages?**

   Yes, MLS provides the same security properties for both group and direct messages. In fact, MLS is particularly efficient for group messaging.

6. **What if a user suspects their wallet is compromised?**

   A wallet key and an installation's MLS keys have different roles. If a wallet is compromised, use the recovery identity to remove that identity and revoke unauthorized installations. Forward secrecy protects past messages when their old encryption keys have been deleted. Changing wallets alone does not revoke existing installations.

7. **How does encryption work across different XMTP apps?**

   All XMTP apps use the same MLS protocol, ensuring consistent encryption across the ecosystem regardless of which app users choose.
