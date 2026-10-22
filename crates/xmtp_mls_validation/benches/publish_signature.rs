//! Measure what publish signatures cost: one admission verification, and the
//! self-checked signing and sealing a client runs before it publishes.
//!
//! A one-off measurement, not a CI check. Run from the repository root, and
//! record the numbers with the CPU they ran on:
//! `dev/nix-shell 'dev/agent-run cargo bench -p xmtp_mls_validation --bench publish_signature'`.
//!
//! - `verify_strict`: the canonical-key check and `verify_strict` over a
//!   prebuilt `GroupMessageTBS`, the rule every verifier shares.
//! - `admit_group_message`: what admission does per signed group message,
//!   building the TBS from the message's fields and verifying it.
//! - `admit_sealed_entry`: the same for a sealed commit-log entry, through
//!   `verify_publish`.
//! - `admit_1000_sealed_entries`: one request at the 1000-envelope bound, all
//!   distinct, verified one after another as the backend's blocking task does.
//! - `sign_once`: one plain Ed25519 signing, the cost a client paid before the
//!   self-check.
//! - `sign_group_message`: signing with the self-check (sign twice, verify).
//! - `seal_commit_log_entry`: sealing with the self-check (nonce, encrypt, sign
//!   twice, verify, decrypt, nonce again). Its self-check cost is roughly this
//!   minus `sign_once` and the AEAD.
//! - `open_commit_log_entry`: what a commit-log reader does per entry.

use criterion::{Criterion, criterion_group, criterion_main};
use ed25519_dalek::{Signer, SigningKey};
use prost::Message;
use std::hint::black_box;
use xmtp_mls_common::group_keys::{
    GroupKeys, GroupRoot, group_message_tbs, open_commit_log_entry, seal_commit_log_entry,
    sign_group_message, verify_publish_signature,
};
use xmtp_mls_validation::{ParsedEnvelope, parse_envelope, verify_publish};
use xmtp_proto::xmtp::{
    backend::v1::{ClientEnvelope, GroupMessage, client_envelope::Payload},
    mls::message_contents::PlaintextCommitLogEntry,
};

/// The size of the group message measured: a typical application message.
const MESSAGE_BYTES: usize = 1024;

fn entry(keys: &GroupKeys, commit_sequence_id: u64) -> Vec<u8> {
    PlaintextCommitLogEntry {
        group_id: keys.group_id().to_vec(),
        commit_sequence_id,
        last_epoch_authenticator: vec![0x60; 32],
        commit_result: 1,
        applied_epoch_number: 8,
        applied_epoch_authenticator: vec![0x62; 32],
    }
    .encode_to_vec()
}

fn parsed_entry(keys: &GroupKeys, commit_sequence_id: u64) -> ParsedEnvelope {
    let sealed = seal_commit_log_entry(keys, &entry(keys, commit_sequence_id)).expect("seal");
    parse_envelope(ClientEnvelope {
        payload: Some(Payload::SealedCommitLogEntry(sealed)),
    })
    .expect("a sealed entry parses")
}

fn bench_publish_signature(c: &mut Criterion) {
    let keys = GroupKeys::derive(&GroupRoot::from_bytes(&[7; 32]).expect("root"));
    let group_id = keys.group_id();
    let mut message = GroupMessage {
        data: vec![0x5a; MESSAGE_BYTES],
        sender_hmac: vec![0x33; 32],
        should_push: true,
        signature: Vec::new(),
    };
    sign_group_message(&keys, &mut message).expect("sign");
    let tbs = group_message_tbs(
        &group_id,
        &message.data,
        &message.sender_hmac,
        message.should_push,
    )
    .expect("tbs");
    let plain = SigningKey::from_bytes(&[9; 32]);
    let plaintext = entry(&keys, 7);
    let sealed = seal_commit_log_entry(&keys, &plaintext).expect("seal");
    let parsed = parsed_entry(&keys, 7);
    let batch: Vec<_> = (1..=1000).map(|id| parsed_entry(&keys, id)).collect();

    let mut group = c.benchmark_group("publish_signature");
    group.bench_function("verify_strict", |b| {
        b.iter(|| {
            verify_publish_signature(black_box(&group_id), black_box(&tbs), &message.signature)
                .expect("verifies")
        })
    });
    group.bench_function("admit_group_message", |b| {
        b.iter(|| {
            let message = black_box(&message);
            let tbs = group_message_tbs(
                &group_id,
                &message.data,
                &message.sender_hmac,
                message.should_push,
            )
            .expect("tbs");
            verify_publish_signature(&group_id, &tbs, &message.signature).expect("verifies")
        })
    });
    group.bench_function("admit_sealed_entry", |b| {
        b.iter(|| verify_publish(black_box(&parsed)).expect("verifies"))
    });
    group.sample_size(10);
    group.bench_function("admit_1000_sealed_entries", |b| {
        b.iter(|| {
            for parsed in black_box(&batch) {
                verify_publish(parsed).expect("verifies");
            }
        })
    });
    group.sample_size(100);
    group.bench_function("sign_once", |b| b.iter(|| plain.sign(black_box(&tbs))));
    group.bench_function("sign_group_message", |b| {
        b.iter(|| {
            let mut message = black_box(message.clone());
            sign_group_message(&keys, &mut message).expect("sign");
            message
        })
    });
    group.bench_function("seal_commit_log_entry", |b| {
        b.iter(|| seal_commit_log_entry(&keys, black_box(&plaintext)).expect("seal"))
    });
    group.bench_function("open_commit_log_entry", |b| {
        b.iter(|| open_commit_log_entry(&keys, black_box(&sealed)).expect("open"))
    });
    group.finish();
}

criterion_group!(benches, bench_publish_signature);
criterion_main!(benches);
