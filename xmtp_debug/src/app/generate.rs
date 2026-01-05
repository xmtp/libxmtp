use crate::{app::App, args};
mod groups;
mod identity;
mod messages;

pub use groups::*;
pub use identity::*;
pub use messages::*;

use color_eyre::eyre::Result;

#[derive(Debug)]
pub struct Generate {
    opts: args::Generate,
    network: args::BackendOpts,
}

impl Generate {
    pub fn new(opts: args::Generate, network: args::BackendOpts) -> Self {
        Self { opts, network }
    }

    pub async fn run(self) -> Result<()> {
        use args::EntityKind::*;
        let Generate { opts, network } = self;
        let args::Generate {
            entity,
            amount,
            invite,
            message_opts,
            concurrency,
            ryow,
            ..
        } = opts;

        info!(?concurrency, "using concurrency");

        match entity {
            Group => {
                let db = App::db()?;
                GenerateGroups::new(db, network)
                    .create_groups(amount, invite.unwrap_or(0), *concurrency)
                    .await?;
                info!("groups generated");
                Ok(())
            }
            Message => {
                GenerateMessages::new(network, message_opts, *concurrency)?
                    .run(amount)
                    .await?;
                info!("messages generated");
                Ok(())
            }
            Identity => {
                let db = App::db()?;
                GenerateIdentity::new(db.into(), network)
                    .create_identities(amount, *concurrency, ryow)
                    .await?;
                info!("identities generated");
                Ok(())
            }
        }
    }
}
