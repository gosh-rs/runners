// imports

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*imports][imports:1]]
use std::path::PathBuf;
use structopt::StructOpt;

use duct::cmd;

use runners::common::*;
// imports:1 ends here

// structopt

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*structopt][structopt:1]]
/// A local runner that can make graceful exit
#[derive(StructOpt, Debug)]
#[structopt(name = "runner", about = "local runner")]
struct Runner {
    #[structopt(flatten)]
    verbosity: Verbosity,

    /// The script file to be run.
    #[structopt(name = "script file", parse(from_os_str))]
    script_file: PathBuf,

    /// Job timeout in seconds
    #[structopt(long = "timeout", short = "t")]
    timeout: Option<u64>,
}
// structopt:1 ends here

// crossbeam

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*crossbeam][crossbeam:1]]
use std::process;
use std::time::Duration;

use crossbeam_channel as cbchan;
use ctrlc;

fn ctrlc_channel() -> Result<cbchan::Receiver<()>> {
    let (sender, receiver) = cbchan::bounded(1);
    ctrlc::set_handler(move || {
        let _ = sender.send(());
    })?;

    Ok(receiver)
}

fn runcmd_channel(fscript: &PathBuf) -> Result<cbchan::Receiver<u32>> {
    let (sender, receiver) = cbchan::bounded(1);

    let p = format!("{}", fscript.display());

    std::thread::spawn(move || {
        // run script in a new session group
        let mut child = process::Command::new("setsid")
            .arg("-w")
            .arg(&p)
            .spawn()
            .expect("failed to execute child");

        let pid = child.id();
        let _ = sender.send(pid);
        let ecode = child.wait().expect("failed to wait on child");
        dbg!(ecode);

        // normal termination
        let _ = sender.send(0);
    });

    Ok(receiver)
}

fn run(args: &Runner) -> Result<()> {
    dbg!(args);

    let ctrl_c_events = ctrlc_channel()?;
    let runcmd_events = runcmd_channel(&args.script_file)?;

    // timeoutlimit control
    let duration = if let Some(t) = args.timeout {
        Some(Duration::from_secs(t))
    } else {
        None
    };

    // Create a channel that times out after the specified duration.
    let timeout = duration
        .map(|d| cbchan::after(d))
        .unwrap_or(cbchan::never());

    // user interruption
    let mut kill = false;
    let mut session_id = 0;
    println!("Press Ctrl-C to stop ...");
    loop {
        cbchan::select! {
            recv(ctrl_c_events) -> _ => {
                println!("User Interrupted.");
                kill = true;
                break;
            }
            recv(runcmd_events) -> msg => {
                match msg {
                    Ok(0) => {
                        println!("Normal Termination.");
                        kill = false;
                        break;
                    }
                    Ok(pid) => {
                        info!("script session id: {}", pid);
                        session_id = pid;
                    }
                    _ => unreachable!()
                }
            }
            recv(timeout) -> _ => {
                println!("Job reaches timeout ...");
                kill = true;
                break;
            },
        }
    }

    if kill {
        println!("Kill running processes ... ");
        if session_id > 0 {
            kill_by_session_id(session_id)?;
        } else {
            kill_child_processes()?;
        }
    }

    Ok(())
}
// crossbeam:1 ends here

// utils

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*utils][utils:1]]
/// kill child processes based on pstree cmd, potentially dangerous
fn kill_child_processes() -> Result<()> {
    let pid = format!("{}", process::id());

    // hide threads using -T option
    let output = cmd!("pstree", "-plT", &pid)
        .pipe(cmd!("grep", "([[:digit:]]*)", "-o"))
        .pipe(cmd!("tr", "-d", "()"))
        .read()?;

    let mut sub_pids: std::collections::HashSet<_> = output.split_whitespace().collect();
    // remove main process id from process list
    sub_pids.remove(pid.as_str());

    if !sub_pids.is_empty() {
        cmd("kill", &sub_pids)
            .unchecked()
            .then(cmd!("pstree", "-pagTl", &pid))
            .run()?;
    }

    Ok(())
}

/// kill child processes by session id
fn kill_by_session_id(sid: u32) -> Result<()> {
    // cmdline: kill -- $(ps -s $1 -o pid=)
    let output = cmd!("ps", "-s", format!("{}", sid), "-o", "pid=").read()?;
    let pids: Vec<_> = output.split_whitespace().collect();

    let mut args = vec!["--"];
    args.extend(&pids);
    if !pids.is_empty() {
        cmd("kill", &args).unchecked().run()?;
    } else {
        info!("No remaining processes found!");
    }

    Ok(())
}
// utils:1 ends here

// main

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*main][main:1]]
fn main() -> Result<()> {
    let args = Runner::from_args();
    args.verbosity.setup_env_logger(&env!("CARGO_PKG_NAME"))?;

    // show program status
    let app_name = format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"),);

    println!("{} starts at {}", app_name, timestamp_now());

    run(&args)?;

    println!("{} completes at {}", app_name, timestamp_now());

    Ok(())
}
// main:1 ends here
