// [[file:~/Workspace/Programming/gosh-rs/runner/runners.note::*imports][imports:1]]
use gosh_core::gut;
use gut::prelude::*;

use structopt::StructOpt;
// imports:1 ends here

// [[file:~/Workspace/Programming/gosh-rs/runner/runners.note::*main][main:1]]
use std::path::Path;
use structopt::*;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    assert!(args.len() >= 1, "{:?}", args);
    // The path to symlink file that invoking the real program
    let invoke_path: &Path = &args[0].as_ref();

    // check file extension for sure (should be foo.run)
    // REVIEW: must be carefully here: not to enter infinite loop
    match invoke_path.extension().and_then(|s| s.to_str()) {
        // apply symlink magic
        // call the program that symlink pointing to
        Some("run") => {
            let invoke_exe = invoke_path.file_stem().context("invoke exe name")?;

            // The path to real executable binary
            let real_path = std::env::current_exe().context("Failed to get exe path")?;
            println!("Runner exe path: {:?}", real_path);
            let real_exe = real_path.file_name().context("real exe name")?;

            let runner_args = [
                &real_exe.to_string_lossy(),
                "-v",
                "--",
                &invoke_exe.to_string_lossy(),
            ];

            let cmdline: Vec<_> = runner_args
                .iter()
                .map(|s| s.to_string())
                .chain(args.iter().cloned().skip(1))
                .collect();
            println!("runner will call {:?} with {:?}", invoke_exe, cmdline.join(" "));
            assert_ne!(invoke_exe, real_exe);

            gosh_runner::Runner::enter_main(cmdline)
        }
        // run in a normal way
        _ => gosh_runner::Runner::enter_main(std::env::args()),
    }
}
// main:1 ends here
