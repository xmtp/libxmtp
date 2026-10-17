//! Measure conversation queries on a fixed local data set.
//!
//! Run from the repository root:
//! `dev/nix-shell 'dev/agent-run cargo bench -p xmtp_db --features bench --bench conversation_list'`.
//! Set `XMTP_BENCH_CONVERSATIONS=50000` for a larger data set.
//! The group uses ten samples. Change `group.sample_size(10)` below for more samples.
//! Setup and database creation are outside the timed operations.

#[path = "support/conversation_fixture.rs"]
mod fixture;

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use xmtp_db::group::{GroupQueryArgs, GroupQueryOrderBy};
use xmtp_db::prelude::*;
use xmtp_db::proto::types::GroupId;

fn bench_conversation_list(c: &mut Criterion) {
    let count = std::env::var("XMTP_BENCH_CONVERSATIONS")
        .map(|value| value.parse::<usize>().expect("conversation count"))
        .unwrap_or(10_000);
    let dir = std::env::temp_dir().join(format!(
        "xmtp-conversations-{}-{}",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    std::fs::create_dir_all(&dir).expect("create benchmark directory");
    let path = dir.join("conversations.db3");
    let store = fixture::open(path.to_str().expect("database path"));
    fixture::seed(&store, count);
    let db = store.db();
    let mut group = c.benchmark_group(format!("conversation_list/{count}"));
    group.sample_size(10);
    for (name, args) in fixture::cases() {
        group.bench_function(format!("list/{name}"), |b| {
            b.iter(|| black_box(db.fetch_conversation_list(black_box(&args)).expect("list")));
        });
        group.bench_function(format!("find_groups/{name}"), |b| {
            b.iter(|| black_box(db.find_groups(black_box(&args)).expect("find groups")));
        });
    }
    group.bench_function("list/created_pages_5", |b| {
        b.iter(|| {
            let mut before = None;
            for _ in 0..5 {
                let args = GroupQueryArgs {
                    limit: Some(fixture::PAGE_SIZE),
                    created_before_ns: before,
                    ..Default::default()
                };
                let rows = db.fetch_conversation_list(&args).expect("list page");
                before = rows.last().map(|row| row.created_at_ns);
                black_box(rows);
            }
        });
    });
    group.bench_function("find_groups/created_pages_5", |b| {
        b.iter(|| {
            let mut after = None;
            for _ in 0..5 {
                let args = GroupQueryArgs {
                    limit: Some(fixture::PAGE_SIZE),
                    created_after_ns: after,
                    order_by: Some(GroupQueryOrderBy::CreatedAt),
                    ..Default::default()
                };
                let rows = db.find_groups(&args).expect("group page");
                after = rows.last().map(|row| row.created_at_ns);
                black_box(rows);
            }
        });
    });
    let id = GroupId::try_from(hex::decode(format!("{:032x}", count / 2)).expect("group id bytes"))
        .expect("group id");
    group.bench_function("find_group", |b| {
        b.iter(|| black_box(db.find_group(black_box(&id)).expect("find group")));
    });
    group.finish();
    drop(db);
    drop(store);
    for suffix in ["", "-wal", "-shm", "-journal", ".sqlcipher_salt"] {
        let file = format!("{}{suffix}", path.display());
        if std::path::Path::new(&file).exists() {
            std::fs::remove_file(file).expect("remove benchmark file");
        }
    }
    std::fs::remove_dir(dir).expect("remove benchmark directory");
}

criterion_group!(benches, bench_conversation_list);
criterion_main!(benches);
