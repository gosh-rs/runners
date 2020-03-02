// imports

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*imports][imports:1]]
use std::fs;
use std::path::PathBuf;
use structopt::StructOpt;

use duct::cmd;

use runners::common::*;
// imports:1 ends here

// structopt

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*structopt][structopt:1]]
#[derive(StructOpt, Debug)]
#[structopt(name = "adhoc", about = "adhoc runner")]
struct AdhocRunner {
    #[structopt(flatten)]
    verbosity: Verbosity,

    /// The job directory to source files
    #[structopt(name = "Job dir", parse(from_os_str))]
    job_dir: PathBuf,

    /// An unique code for current job
    #[structopt(name = "Job code")]
    job_code: String,

    /// Path to trajectory file relative to work directory (job_code)
    #[structopt(name = "Trajectory filename")]
    trj_file: String,

    /// version tag for calling rxe, e.g.: v0.0.30 => rxe-v0.0.30
    #[structopt(name = "version tag")]
    exe_tag: Option<String>,

    /// Set spring constant for all springs.
    #[structopt(short = "k", default_value = "1.0")]
    k: String,

    /// Set the algorithm for refining reaction path. Possible choices: NEB, PEB, DEB.
    #[structopt(name = "METHOD", long = "scheme", short = "s")]
    scheme: String,

    /// The max allowed force vector norm for image optimization.
    #[structopt(long = "fmax", default_value = "0.1")]
    fmax: String,

    /// Append results to existing log file
    #[structopt(long = "append")]
    append: bool,
}
// structopt:1 ends here

// core

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*core][core:1]]
fn adhoc(args: &AdhocRunner) -> Result<()> {
    dbg!(args);

    // construct exe
    let rxe = if let Some(ref v) = args.exe_tag {
        format!("rxe-{}", v)
    } else {
        format!("rxe")
    };

    // prepare work directory
    let mut wdir = args.job_dir.clone();
    wdir.push(&args.job_code);
    fs::create_dir_all(&wdir)?;

    // conditional constants
    let fout = format!("{}.xyz", args.scheme);

    // construct cmdline
    let cmdline = cmd!(
        rxe,
        "-t",
        "../bbm",
        "refine",
        &args.trj_file,
        "-s",
        &args.scheme,
        "-o",
        fout,
        "--fmax",
        &args.fmax,
        "-k",
        &args.k
    ).dir(&wdir);

    dbg!(&cmdline);

    // keep job results
    let tee = if args.append {
        cmd!("tee", "-a", "runner.log")
    } else {
        cmd!("tee", "runner.log")
    }.dir(&wdir);

    // run it
    cmdline.stderr_to_stdout().pipe(tee).run()?;

    dbg!(wdir);

    Ok(())
}
// core:1 ends here

// main

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*main][main:1]]
fn main() -> Result<()> {
    let args = AdhocRunner::from_args();
    args.verbosity.setup_env_logger(&env!("CARGO_PKG_NAME"))?;

    // show program status
    let app_name = format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"),);

    println!("{} starts at {}", app_name, timestamp_now());

    adhoc(&args)?;

    println!("{} completes at {}", app_name, timestamp_now());

    Ok(())
}
// main:1 ends here
