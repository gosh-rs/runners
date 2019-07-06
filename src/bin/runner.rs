// imports

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*imports][imports:1]]
use structopt::StructOpt;
use runners::local::*;
use runners::common::*;
// imports:1 ends here

// main

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*main][main:1]]
/// A local runner that can make graceful exit
#[derive(StructOpt, Debug)]
pub struct Cli {
    #[structopt(flatten)]
    verbosity: Verbosity,

    #[structopt(flatten)]
    app: Runner,
}

fn main() -> Result<()> {
    let args = Cli::from_args();
    args.verbosity.setup_env_logger(&env!("CARGO_PKG_NAME"))?;

    args.app.run()?;

    Ok(())
}
// main:1 ends here
