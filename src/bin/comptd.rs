// main/warp

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*main/warp][main/warp:1]]
use runners::common::*;
use runners::serv_warp::*;

/// A local runner that can make graceful exit
#[derive(StructOpt, Debug)]
pub struct Cli {
    #[structopt(flatten)]
    verbosity: Verbosity,
}

fn main() -> Result<()> {
    let args = Cli::from_args();
    args.verbosity.setup_env_logger(&env!("CARGO_PKG_NAME"))?;

    test();

    Ok(())
}
// main/warp:1 ends here
