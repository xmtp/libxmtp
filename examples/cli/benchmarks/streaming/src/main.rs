use anyhow::Result;
use clap::Parser;
use futures::StreamExt;
use tokio::{runtime::Runtime, sync::oneshot};
use tracing::{error, info};
use xmtp_mls::tester;

#[derive(Parser)]
struct Args {}

fn main() -> Result<()> {
    let args = Args::parse();

    let inbox_id = setup_monitor()?;
    info!("Monitor inbox_id: {inbox_id}");

    Ok(())
}

fn setup_monitor() -> Result<String> {
    let (tx, rx) = oneshot::channel();

    std::thread::spawn(move || {
        let rt = Runtime::new()?;
        rt.block_on(async move {
            tester!(andre);
            tx.send(andre.inbox_id().to_string());

            let stream = andre.stream_all_messages(None, None).await?;
            while let Some(Ok(msg)) = stream.next().await {}
        });
    });

    Ok(rx.blocking_recv()?)
}
