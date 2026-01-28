//! Tests for heap profile high water marks
//! each memtest must be in its own integration test file to avoid
//! interference and false statistics
//!
use std::{io::pipe, path::PathBuf, process::Command};

use alloy::primitives::Address;
use color_eyre::eyre::{OptionExt, Result, eyre};
use dhat::HeapStats;
use xmtp_id::associations::{Identifier, ident};
use xmtp_mls::tester;
use xshell::{Shell, cmd};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn get_ids() -> Result<Vec<Address>> {
    let sh = Shell::new()?;
    let xdbg = xdbg()?;
    let (in_reader, in_writer) = pipe()?;
    let (out_reader, out_writer) = pipe()?;

    tracing::info!("getting ids");
    let mut xdbg_cmd: Command = cmd!(sh, "{xdbg} export --entity identity").into();
    let mut xdbg_child = xdbg_cmd.stdout(in_writer).spawn()?;

    let _jq = Command::new("jq")
        .arg("[.[].ethereum_address]")
        .stdin(in_reader)
        .stdout(out_writer)
        .spawn()?;
    drop(xdbg_cmd);
    xdbg_child.wait()?;
    serde_json::from_reader(&out_reader).map_err(Into::into)
}

// use xdbg to generate `size` identities
// using xdbg as to not effect the memory calculations of `dhat`, since `xdbg` is out of this process.
// it will also conveniently re-use identities if they already exist.
fn create_watchers(size: usize) -> Result<Vec<Address>> {
    let sh = Shell::new()?;
    let xdbg = xdbg()?;

    let ids = get_ids()?;
    if ids.len() >= size {
        return Ok(ids.into_iter().take(size).collect());
    }
    let size_str = (size - ids.len()).to_string();
    let mut c: Command = cmd!(sh, "{xdbg} generate --entity identity --amount {size_str}").into();
    let mut c = c.spawn()?;
    c.wait()?;
    let ids: Vec<_> = get_ids()?;
    if ids.len() < size {
        return Err(eyre!("not enough ids"));
    }
    Ok(ids)
}

/// Get xdbg binary from /target if it exists, otherwise compile it then get it.
fn xdbg() -> Result<PathBuf> {
    let sh = Shell::new()?;
    let workspace = cmd!(
        sh,
        "cargo locate-project --workspace --message-format=plain"
    )
    .read()?;
    let xdbg = PathBuf::from(workspace)
        .parent()
        .ok_or_eyre("no parent of libxmtp manifest")?
        .join("target")
        .join("release")
        .join("xdbg");
    if !xdbg.exists() {
        cmd!(sh, "cargo build --release -p xdbg").run()?;
    }
    Ok(xdbg)
}

/// ensure dm broadcasting uses a reasonable amount of memory
#[tokio::test]
async fn memtest_dm_broadcast() -> Result<()> {
    let _profiler = dhat::Profiler::builder().testing().build();

    // fork out to xdbg to create/reuse clients, and run cmds out of the process,
    // minimizing any memory the allocator picks up from test setup.
    let ids = create_watchers(300)?;
    tracing::info!("got {} ids", ids.len());

    for watcher in ids {
        // create a new client for each broadcast
        tester!(broadcaster);
        let dm = broadcaster
            .find_or_create_dm_by_identity(
                Identifier::Ethereum(ident::Ethereum(watcher.to_string().to_ascii_lowercase())),
                None,
            )
            .await?;
        dm.send_message(b"BROADCAST MESSAGE", Default::default())
            .await?;
    }
    // ensure that dm broadcast stayed under 100MB
    let stats = HeapStats::get();
    println!("{:?}", stats);
    dhat::assert!(stats.max_bytes < (100 * 1024 * 1024));
    Ok(())
}
