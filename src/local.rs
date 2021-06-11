// [[file:../runners.note::*imports][imports:1]]
use crate::common::*;
use crate::session::Session;
// imports:1 ends here

// [[file:../runners.note::*cli][cli:1]]
use gut::cli::*;

/// A local runner that can make graceful exit
#[derive(StructOpt, Debug, Default)]
struct RunnerCli {
    #[structopt(flatten)]
    verbose: gut::cli::Verbosity,

    /// Job timeout in seconds. The default timeout is 30 days.
    #[structopt(long = "timeout", short = "t")]
    timeout: Option<u32>,

    /// Command line to call a program
    #[structopt(raw = true, required = true)]
    cmdline: Vec<String>,
}

impl RunnerCli {
    fn enter_main<I>(iter: I) -> Result<()>
    where
        Self: Sized,
        I: IntoIterator,
        I::Item: Into<std::ffi::OsString> + Clone,
    {
        let args = RunnerCli::from_iter_safe(iter)?;
        args.verbose.setup_logger();

        let program = &args.cmdline[0];
        let rest = &args.cmdline[1..];

        Session::new(program)
            .args(rest)
            .timeout(args.timeout.unwrap_or(3600 * 24 * 30))
            .run()?;

        Ok(())
    }
}

pub fn enter_main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    assert!(args.len() >= 1, "{:?}", args);
    // The path to symlink file that invoking the real program
    let invoke_path: &Path = &args[0].as_ref();

    // check file extension for sure (should be foo.run)
    // REVIEW: must be carefully here: not to enter infinite loop
    if let Some("run") = invoke_path.extension().and_then(|s| s.to_str()) {
        // apply symlink magic
        // call the program that symlink pointing to
        let invoke_exe = invoke_path.file_stem().context("invoke exe name")?;

        // The path to real executable binary
        let real_path = std::env::current_exe().context("Failed to get exe path")?;
        println!("Runner exe path: {:?}", real_path);
        let real_exe = real_path.file_name().context("real exe name")?;

        if real_exe != invoke_exe {
            let runner_args = [&real_exe.to_string_lossy(), "-v", "--", &invoke_exe.to_string_lossy()];

            let cmdline: Vec<_> = runner_args
                .iter()
                .map(|s| s.to_string())
                .chain(args.iter().cloned().skip(1))
                .collect();
            println!("runner will call {:?} with {:?}", invoke_exe, cmdline.join(" "));
            return RunnerCli::enter_main(cmdline);
        }
    }
    // run in a normal way
    RunnerCli::enter_main(std::env::args())
}
// cli:1 ends here
