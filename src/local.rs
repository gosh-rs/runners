use std::path::{Path, PathBuf};
use std::process;
use std::time::Duration;
use structopt::StructOpt;

use crate::common::*;

/// A local runner that can make graceful exit
#[derive(StructOpt, Debug, Default)]
pub struct Runner {
    /// The program to be run.
    #[structopt(name = "program", parse(from_os_str))]
    program: PathBuf,

    /// Input stream in stdin.
    #[structopt(long = "input")]
    input: Option<String>,

    /// Job timeout in seconds
    #[structopt(long = "timeout", short = "t")]
    timeout: Option<u64>,

    /// Arguments that will be passed to `program`
    #[structopt(raw = true)]
    rest: Vec<String>,
}

impl Runner {
    /// Run program
    pub fn run(&self) -> Result<()> {
        run(&self)
    }

    pub fn new<P: AsRef<Path>>(program: P) -> Self {
        Self {
            program: program.as_ref().into(),
            ..Default::default()
        }
    }

    /// Set program argument
    pub fn arg<S: AsRef<str>>(mut self, arg: S) -> Self {
        self.rest.push(arg.as_ref().into());
        self
    }

    /// Set runner timeout
    pub fn timeout(mut self, t: u64) -> Self {
        self.timeout = Some(t);
        self
    }

    /// Set runner input
    pub fn input(mut self, inp: String) -> Self {
        self.input = Some(inp);
        self
    }

    /// Spawn child process in a new session.
    pub fn build_command(&self) -> std::process::Command {
        let mut cmd = process::Command::new("setsid");
        cmd.arg("-w").arg(&self.program).args(&self.rest);
        cmd
    }
}

use duct::cmd;

// FIXME: remove
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

    // if !sub_pids.is_empty() {
    //     cmd("kill", &sub_pids)
    //         .unchecked()
    //         .then(cmd!("pstree", "-pagTl", &pid))
    //         .run()?;
    // }

    Ok(())
}

/// terminate child processes in a session.
pub fn terminate_session(sid: u32) -> Result<()> {
    signal_processes_by_session_id(sid, "SIGTERM")
}

/// Kill processes in a session.
pub fn kill_session(sid: u32) -> Result<()> {
    // signal_processes_by_session_id(sid, "SIGKILL")
    kill_child_processes_by_session_id(sid)
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

/// Find child processes using psutil (without using shell commands)
///
/// # Reference
///
/// https://github.com/borntyping/rust-psutil/blob/master/examples/ps.rs
fn kill_child_processes_by_session_id(sid: u32) -> Result<()> {
    if let Ok(processes) = psutil::process::all() {
        // collect pids then kill
        let child_processes: Vec<_> = processes
            .into_iter()
            .filter(|p| p.session == sid as i32)
            .inspect(|p| {
                dbg!(p.session);
            })
            .collect();

        for p in child_processes {
            p.kill()?;
        }
    }
    Ok(())
}
