// imports

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*imports][imports:1]]
use std::path::PathBuf;
use structopt::StructOpt;

use duct::cmd;

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

// crossbeam

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*crossbeam][crossbeam:1]]
use std::process;
use std::time::Duration;

use crate::in_temp_dir;
use crossbeam_channel as cbchan;
use ctrlc;

fn ctrlc_channel() -> Result<cbchan::Receiver<()>> {
    let (sender, receiver) = cbchan::bounded(1);
    ctrlc::set_handler(move || {
        let _ = sender.send(());
    })?;

    Ok(receiver)
}

fn runcmd_channel(fscript: &PathBuf, cmd_args: Vec<String>) -> Result<cbchan::Receiver<u32>> {
    let (sender, receiver) = cbchan::bounded(1);

    let p = format!("{}", fscript.display());
    std::thread::spawn(move || {
        in_temp_dir!({
            let mut child = process::Command::new("setsid")
                .arg("-w")
                .arg(p)
                .args(cmd_args)
                .spawn()
                .expect("failed to execute child");

            let pid = child.id();
            let _ = sender.send(pid);
            let ecode = child.wait().expect("failed to wait on child");
            if !ecode.success() {
                error!("program exits with failure!");
                dbg!(ecode);
            }

            // normal termination
            let _ = sender.send(0);
        });
    });

    Ok(receiver)
}

pub fn run(args: &Runner) -> Result<()> {
    // show program status
    let app_name = format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"),);

    println!("{} starts at {}", app_name, timestamp_now());
    dbg!(args);

    let ctrl_c_events = ctrlc_channel()?;

    let runcmd_events = runcmd_channel(&args.program, args.rest.clone())?;

    // timeout control
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
                println!("User interrupted.");
                kill = true;
                break;
            }
            recv(runcmd_events) -> msg => {
                match msg {
                    Ok(0) => {
                        println!("Job completed.");
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
            terminate_session(session_id)?;
        } else {
            kill_child_processes()?;
        }
    }
    println!("{} completes at {}", app_name, timestamp_now());

    Ok(())
}
// crossbeam:1 ends here

// utils

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*utils][utils:1]]
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
