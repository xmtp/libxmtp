use std::fs::File;
use std::io::Read;
use std::sync::Arc;

use crate::{
    app::{self, clients},
    args,
};
use color_eyre::eyre::Result;
use futures::stream::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};

use super::{export::IdentityExport, types::Identity};

#[derive(Debug)]
pub struct Stream {
    _db: Arc<redb::Database>,
    opts: args::Stream,
    network: args::BackendOpts,
}

impl Stream {
    pub fn new(opts: args::Stream, network: args::BackendOpts, db: Arc<redb::Database>) -> Self {
        Self {
            opts,
            network,
            _db: db,
        }
    }

    pub async fn run(self) -> Result<()> {
        let args::Stream { ref import } = self.opts;

        // setup the identity
        let identity: Identity = {
            let mut file = File::open(import)?;
            let mut s = String::new();
            file.read_to_string(&mut s)?;
            let json: IdentityExport = miniserde::json::from_str(&s)?;
            let identity: Identity = json.try_into()?;
            // create a new installation
            let _ =
                clients::new_installation_from_identity(identity.clone(), &self.network).await?;
            identity
        };

        let client = clients::client_from_identity(&identity, &self.network).await?;
        info!("Streaming for inbox_id={}", client.inbox_id());
        let mut stream = client.stream_all_messages(None).await?;
        let mut stream = std::pin::pin!(stream);
        let style =
            ProgressStyle::with_template("{spinner} streamed {pos} messages in {elapsed}").unwrap();
        let bar = ProgressBar::no_length().with_style(style);

        let mut total_messages_streamed = 0;
        // TODO: Record streamed messages in database
        // allow JSON export for inspection/asserts
        let duration = std::time::Duration::from_millis(45);
        loop {
            if app::App::is_terminated() {
                break;
            }
            tokio::select! {
                _ = app::App::is_terminated_future() => {
                    break;
                },
                _ = tokio::time::sleep(duration) => {
                    bar.tick()
                },
                m = stream.next() => {
                    match m {
                        Some(Ok(_msg)) => {
                            bar.inc(1);
                            total_messages_streamed += 1;
                        }
                        Some(Err(e)) => {
                            error!("{}", e);
                        },
                        None => break,
                    }
                }
            }
        }
        app::App::write_diagnostic(format!("{}", total_messages_streamed))?;
        Ok(())
    }
}

// list of refs in nix to build xdbg for
// spit out binaries
// script can loop over binaries
// create identities in different databases
// one stream should receive all messages
