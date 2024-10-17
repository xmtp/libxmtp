mod build;
mod cmds;

use color_eyre::eyre::Result;
use std::env;

pub use cmds::flags;
// pub const WASM_RUSTFLAGS: &str = "-Ctarget-feature=+bulk-memory,+mutable-globals";

pub mod tasks {
    use super::*;
    use crate::build;

    pub fn build(flags: flags::Build) -> Result<()> {
        let extra_args: Vec<String> = env::args().skip_while(|x| x != "--").skip(1).collect();
        build::build(&extra_args, flags)
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let args = std::env::args_os().skip(1).take_while(|e| e != "--");
    let app = flags::Libxmtp::from_vec(args.collect());
    let app = match app {
        Err(e) => {
            if e.is_help() {
                print!("{}", e);
                return Ok(());
            } else {
                e.exit()
            }
        }
        Ok(r) => r,
    };
    match app.subcommand {
        flags::LibxmtpCmd::Build(f) => tasks::build(f)?,
    };
    Ok(())
}
