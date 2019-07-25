// main/warp

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*main/warp][main/warp:1]]
use runners::common::*;
use runners::server::*;

/// Application server for remote calculations.
#[derive(StructOpt, Debug)]
pub struct Cli {
    #[structopt(flatten)]
    verbosity: Verbosity,

    /// Set application server address for binding.
    #[structopt(name = "ADDRESS")]
    address: Option<String>,
}

fn main() -> Result<()> {
    let args = Cli::from_args();
    args.verbosity.setup_env_logger(&env!("CARGO_PKG_NAME"))?;

    if let Some(addr) = args.address {
        dbg!(&addr);
        bind(&addr);
    } else {
        run();
    }

    Ok(())
}
// main/warp:1 ends here
