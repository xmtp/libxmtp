"""Group-key test vectors from an implementation independent of libxmtp.

Everything here comes from the spec text alone: the HKDF salt and labels of
the group key table, the GroupMessageTBS and CommitLogEntryTBS structures,
and the commit-log seal. The cryptography comes from pyca/cryptography. The
TLS variable-length vectors (RFC 9420 section 2.1.2) and the protobuf
encoding of the PlaintextCommitLogEntry are written out by hand below.

The values are public test values. Never use them outside tests.

See README.md for the command that regenerates vectors.txt.
"""

from cryptography.hazmat.primitives import hashes, hmac
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.hazmat.primitives.kdf.hkdf import HKDF, HKDFExpand

SALT = b"xmtp group root"
GROUP_SIGNING_KEY_INFO = b"xmtp group signing key"
COMMIT_LOG_SIGNING_KEY_INFO = b"xmtp commit log signing key"
COMMIT_LOG_ENCRYPTION_KEY_INFO = b"xmtp commit log encryption key"
COMMIT_LOG_NONCE_KEY_INFO = b"xmtp commit log nonce key"
GROUP_MESSAGE_LABEL = b"xmtp group message"
COMMIT_LOG_ENTRY_LABEL = b"xmtp commit log entry"
GROUP_MESSAGE_KIND = b"\x00"
COMMIT_LOG_KIND = b"\x04"
NONCE_BYTES = 12

# Inputs. Lengths are chosen so that the ciphertext and the group message
# data each need a two-byte vector length prefix.
ROOT = bytes.fromhex("6a09e667f3bcc908bb67ae8584caa73b3c6ef372fe94f82ba54ff53a5f1d36f1")
COMMIT_SEQUENCE_ID = 1234567
LAST_EPOCH_AUTHENTICATOR = bytes(range(0xA0, 0xC0))
COMMIT_RESULT_APPLIED = 1
APPLIED_EPOCH_NUMBER = 42
APPLIED_EPOCH_AUTHENTICATOR = bytes(range(0xC0, 0xE0))
GROUP_MESSAGE_DATA = bytes((7 * i + 3) % 256 for i in range(100))
GROUP_MESSAGE_SENDER_HMAC = bytes(range(0x50, 0x70))
GROUP_MESSAGE_SHOULD_PUSH = True


def hmac_sha256(key: bytes, data: bytes) -> bytes:
    mac = hmac.HMAC(key, hashes.SHA256())
    mac.update(data)
    return mac.finalize()


def expand(prk: bytes, info: bytes) -> bytes:
    return HKDFExpand(algorithm=hashes.SHA256(), length=32, info=info).derive(prk)


def tls_vector(data: bytes) -> bytes:
    """An opaque<V>: RFC 9420 section 2.1.2 variable-length vector."""
    length = len(data)
    if length < 1 << 6:
        prefix = length.to_bytes(1, "big")
    elif length < 1 << 14:
        prefix = (0x4000 | length).to_bytes(2, "big")
    elif length < 1 << 30:
        prefix = (0x80000000 | length).to_bytes(4, "big")
    else:
        raise ValueError("vector too long")
    return prefix + data


def proto_varint(value: int) -> bytes:
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        if value:
            out.append(byte | 0x80)
        else:
            out.append(byte)
            return bytes(out)


def proto_bytes(field: int, data: bytes) -> bytes:
    return proto_varint(field << 3 | 2) + proto_varint(len(data)) + data


def proto_uint(field: int, value: int) -> bytes:
    return proto_varint(field << 3) + proto_varint(value)


def public_key(seed: bytes) -> bytes:
    return Ed25519PrivateKey.from_private_bytes(seed).public_key().public_bytes_raw()


def sign(seed: bytes, message: bytes) -> bytes:
    return Ed25519PrivateKey.from_private_bytes(seed).sign(message)


def main() -> None:
    # HKDF-Extract(salt, IKM) is HMAC-SHA256 keyed by the salt (RFC 5869).
    prk = hmac_sha256(SALT, ROOT)
    keys = {
        "group_signing_seed": expand(prk, GROUP_SIGNING_KEY_INFO),
        "commit_log_signing_seed": expand(prk, COMMIT_LOG_SIGNING_KEY_INFO),
        "commit_log_encryption_key": expand(prk, COMMIT_LOG_ENCRYPTION_KEY_INFO),
        "commit_log_nonce_key": expand(prk, COMMIT_LOG_NONCE_KEY_INFO),
    }
    # The one-shot HKDF must agree with the two steps above.
    one_shot = HKDF(
        algorithm=hashes.SHA256(), length=32, salt=SALT, info=GROUP_SIGNING_KEY_INFO
    ).derive(ROOT)
    assert one_shot == keys["group_signing_seed"]

    group_id = public_key(keys["group_signing_seed"])
    commit_log_id = public_key(keys["commit_log_signing_seed"])

    # PlaintextCommitLogEntry, fields in number order.
    plaintext = (
        proto_bytes(1, group_id)
        + proto_uint(2, COMMIT_SEQUENCE_ID)
        + proto_bytes(3, LAST_EPOCH_AUTHENTICATOR)
        + proto_uint(4, COMMIT_RESULT_APPLIED)
        + proto_uint(5, APPLIED_EPOCH_NUMBER)
        + proto_bytes(6, APPLIED_EPOCH_AUTHENTICATOR)
    )
    commit_log_topic = COMMIT_LOG_KIND + commit_log_id
    nonce = hmac_sha256(keys["commit_log_nonce_key"], commit_log_topic + plaintext)[
        :NONCE_BYTES
    ]
    ciphertext = ChaCha20Poly1305(keys["commit_log_encryption_key"]).encrypt(
        nonce, plaintext, commit_log_topic
    )
    commit_log_entry_tbs = (
        tls_vector(COMMIT_LOG_ENTRY_LABEL)
        + tls_vector(commit_log_topic)
        + tls_vector(nonce)
        + tls_vector(ciphertext)
    )
    commit_log_entry_signature = sign(
        keys["commit_log_signing_seed"], commit_log_entry_tbs
    )

    group_message_tbs = (
        tls_vector(GROUP_MESSAGE_LABEL)
        + tls_vector(GROUP_MESSAGE_KIND + group_id)
        + tls_vector(GROUP_MESSAGE_DATA)
        + tls_vector(GROUP_MESSAGE_SENDER_HMAC)
        + bytes([1 if GROUP_MESSAGE_SHOULD_PUSH else 0])
    )
    group_message_signature = sign(keys["group_signing_seed"], group_message_tbs)

    vectors = {
        "root": ROOT,
        "prk": prk,
        **keys,
        "group_id": group_id,
        "commit_log_id": commit_log_id,
        "entry_commit_sequence_id": COMMIT_SEQUENCE_ID.to_bytes(8, "big"),
        "entry_last_epoch_authenticator": LAST_EPOCH_AUTHENTICATOR,
        "entry_applied_epoch_number": APPLIED_EPOCH_NUMBER.to_bytes(8, "big"),
        "entry_applied_epoch_authenticator": APPLIED_EPOCH_AUTHENTICATOR,
        "plaintext": plaintext,
        "commit_log_topic": commit_log_topic,
        "nonce": nonce,
        "ciphertext": ciphertext,
        "commit_log_entry_tbs": commit_log_entry_tbs,
        "commit_log_entry_signature": commit_log_entry_signature,
        "group_message_data": GROUP_MESSAGE_DATA,
        "group_message_sender_hmac": GROUP_MESSAGE_SENDER_HMAC,
        "group_message_tbs": group_message_tbs,
        "group_message_signature": group_message_signature,
    }
    print("# Generated by generate_vectors.py. See README.md. Do not edit.")
    for name, value in vectors.items():
        print(f"{name} = {value.hex()}")


if __name__ == "__main__":
    main()
