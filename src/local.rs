// imports

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*imports][imports:1]]
use std::path::PathBuf;
use std::process;
use std::time::Duration;
use structopt::StructOpt;

use crate::common::*;
// imports:1 ends here

// runner

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*runner][runner:1]]
/// A local runner that can make graceful exit
#[derive(StructOpt, Debug)]
pub struct Runner {
    /// The program to be run.
    #[structopt(name = "program", parse(from_os_str))]
    program: PathBuf,

    /// Job timeout in seconds
    #[structopt(long = "timeout", short = "t")]
    timeout: Option<u64>,

    /// Arguments that will be passed to `program`
    #[structopt(raw = true)]
    rest: Vec<String>,
}

impl Runner {
    pub fn run(&self) -> Result<()> {
        run(&self)
    }
}
// runner:1 ends here

// tokio

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*tokio][tokio:1]]
use tokio::prelude::*;
use tokio_process::CommandExt;
use tokio_signal::unix::{Signal, SIGINT, SIGTERM};

pub fn run(args: &Runner) -> Result<()> {
    // show program status
    let app_name = format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"),);
    println!("{} starts at {}", app_name, timestamp_now());
    dbg!(args);

    // Use the standard library's `Command` type to build a process and then
    // execute it via the `CommandExt` trait.
    let child = process::Command::new("setsid")
        .arg("-w")
        .arg(&args.program)
        .args(&args.rest)
        .spawn_async()
        .map_err(|e| {
            error!("Error while constructing command, details:\n {}", e);
            e
        })?;

    let session_id = child.id();
    info!("Job session id: {}", session_id);

    // Create an infinite stream of signal notifications. Each item received on
    // this stream may represent multiple signals.
    let sig_int = Signal::new(SIGINT).flatten_stream().map_err(|e| ());
    let sig_term = Signal::new(SIGTERM).flatten_stream().map_err(|e| ());

    // When timeout, send TERM signal. Default timeout = 30 days
    let t = args.timeout.unwrap_or(3600 * 24 * 30);
    let timeout = Duration::from_secs(t);

    // Use the `select` combinator to merge these streams into one. Process
    // signal as it comes in.
    let signals = sig_int
        .select(sig_term)
        .timeout(timeout)
        .take(1)
        .for_each(move |_| {
            println!("User interrupted.");
            println!("Kill running processes ({}) ... ", session_id);
            terminate_session(session_id).unwrap();
            Ok(())
        })
        .map_err(move |_| {
            error!("Command timeout after {} seconds!", t);
            terminate_session(session_id).unwrap();
        });

    let cmd = child
        .map(|status| println!("exit status: {:?}", status))
        .map_err(|e| error!("cmd failed with errors\n: {}", e));

    tokio::run(signals.select2(cmd).map(|_| ()).map_err(|_| ()));

    Ok(())
}
// tokio:1 ends here

// utils

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*utils][utils:1]]
use duct::cmd;

/// kill child processes based on pstree cmd, which is not very reliable.
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

/// terminate child processes in a session.
pub fn terminate_session(sid: u32) -> Result<()> {
    signal_processes_by_session_id(sid, "SIGTERM")
}

/// Kill processes in a session.
pub fn kill_session(sid: u32) -> Result<()> {
    signal_processes_by_session_id(sid, "SIGKILL")
}

/// Resume processes in a session.
pub fn resume_session(sid: u32) -> Result<()> {
    signal_processes_by_session_id(sid, "SIGCONT")
}

/// Pause processes in a session.
pub fn pause_session(sid: u32) -> Result<()> {
    signal_processes_by_session_id(sid, "SIGSTOP")
}

/// signal processes by session id
fn signal_processes_by_session_id(sid: u32, signal: &str) -> Result<()> {
    // cmdline: kill -CONT -- $(ps -s $1 -o pid=)
    let output = cmd!("ps", "-s", format!("{}", sid), "-o", "pid=").read()?;
    let pids: Vec<_> = output.split_whitespace().collect();

    let mut args = vec!["-s", signal, "--"];
    args.extend(&pids);
    if !pids.is_empty() {
        cmd("kill", &args).unchecked().run()?;
    } else {
        info!("No remaining processes found!");
    }

    Ok(())
}
// utils:1 ends here
