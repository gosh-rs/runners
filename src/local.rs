// imports

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*imports][imports:1]]
use std::path::{Path, PathBuf};
use std::process;
use std::time::Duration;
use structopt::StructOpt;

use crate::common::*;
// imports:1 ends here

// runner

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*runner][runner:1]]
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
// runner:1 ends here

// tokio

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*tokio][tokio:1]]
use tokio::prelude::*;
use tokio_process::CommandExt;
use tokio_signal::unix::{Signal, SIGINT, SIGTERM};

pub(crate) fn run(args: &Runner) -> Result<()> {
    // show program status
    let app_name = format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"),);
    println!("{} starts at {}", app_name, timestamp_now());
    dbg!(args);

    // Use the standard library's `Command` type to build a process and then
    // execute it via the `CommandExt` trait.
    let child = args.build_command().spawn_async().map_err(|e| {
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
            kill_session(session_id).unwrap();
            Ok(())
        })
        .map_err(move |_| {
            error!("Command timeout after {} seconds!", t);
            kill_session(session_id).unwrap();
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
// utils:1 ends here

// crossbeam/bbm/adhoc
// : signal-hook = {version = "0.1", features = ["tokio-support"] }

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*crossbeam/bbm/adhoc][crossbeam/bbm/adhoc:1]]
// FIXME: adhoc hacking
pub use self::adhoc::*;

mod adhoc {
    use super::*;

    use crossbeam_channel as cbchan;
    use signal_hook::{iterator::Signals, SIGCONT, SIGINT, SIGTERM, SIGUSR1};

    /// for cmd channel
    enum CmdResult {
        Pid(u32),
        Output(String),
    }

    pub fn run_adhoc_input_output<P: AsRef<Path>>(
        args: &Runner,
        input: &str,
        current_dir: P,
    ) -> Result<String> {
        let signal_events = signal_channel()?;
        let runcmd_events = runcmd_channel(
            &args.program,
            input,
            args.rest.clone(),
            current_dir.as_ref(),
        )?;

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
        eprintln!("Press Ctrl-C to stop ...");
        let mut cmd_output = String::new();
        loop {
            cbchan::select! {
                recv(signal_events) -> sig => {
                    match sig {
                        Ok(SIGINT) | Ok(SIGTERM) => {
                            eprintln!("Try to gracefully exit ...");
                            kill = true;
                            break;
                        }
                        Ok(SIGCONT) => {
                            eprintln!("Resume calculation ... {:?}", sig);
                            if session_id > 0 {
                                resume_session(session_id)?;
                            } else {
                                error!("Calculation not start yet.");
                            }
                        }
                        Ok(SIGUSR1) => {
                            eprintln!("Pause calculation ... {:?}", sig);
                            if session_id > 0 {
                                pause_session(session_id)?;
                            } else {
                                error!("Calculation not start yet.");
                            }
                        }
                        Ok(sig) => {
                            warn!("unprocessed signal {:?}", sig);
                        }
                        Err(e) => {
                            eprintln!("Process signal hook failed: {:?}", e);
                            break;
                        }
                    }
                }
                recv(runcmd_events) -> msg => {
                    match msg {
                        Ok(CmdResult::Output(o)) => {
                            eprintln!("Job completed.");
                            kill = false;
                            cmd_output = o;
                            break;
                        }
                        Ok(CmdResult::Pid(pid)) => {
                            info!("script session id: {}", pid);
                            session_id = pid;
                        }
                        Err(e) => {
                            error!("found error: {}", e);
                        }
                        _ => unreachable!()
                    }
                }
                recv(timeout) -> _ => {
                    eprintln!("Job reaches timeout ...");
                    kill = true;
                    break;
                },
            }
        }

        if kill {
            eprintln!("Kill running processes ... ");
            if session_id > 0 {
                kill_session(session_id)?;
            } else {
                kill_child_processes()?;
            }
        }

        Ok(cmd_output)
    }

    fn signal_channel() -> Result<cbchan::Receiver<i32>> {
        let signals = Signals::new(&[SIGINT, SIGCONT, SIGTERM, SIGUSR1])?;

        let (sender, receiver) = cbchan::bounded(1);

        std::thread::spawn(move || {
            for sig in signals.forever() {
                let _ = sender.send(sig);
            }
        });

        Ok(receiver)
    }

    fn runcmd_channel(
        fscript: &PathBuf,
        input: &str,
        cmd_args: Vec<String>,
        current_dir: &Path,
    ) -> Result<cbchan::Receiver<CmdResult>> {
        let (sender, receiver) = cbchan::bounded(1);

        let p = format!("{}", fscript.display());
        let input = input.to_owned();
        let cdir = current_dir.to_owned();
        std::thread::spawn(move || {
            let mut child = process::Command::new("setsid")
                .arg("-w")
                .arg(p)
                .args(cmd_args)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .current_dir(cdir)
                .spawn()
                .expect("failed to execute child");

            let pid = child.id();
            info!("Job session id: {}", pid);
            let _ = sender.send(CmdResult::Pid(pid));

            {
                let stdin = child.stdin.as_mut().expect("Failed to open stdin");
                stdin
                    .write_all(input.as_bytes())
                    .expect("Failed to write to stdin");
            }

            let p_output = child.wait_with_output().expect("Failed to read stdout");
            let output = String::from_utf8_lossy(&p_output.stdout);
            let ecode = p_output.status;
            if !ecode.success() {
                error!("program exits with failure!");
                dbg!(ecode);
            }
            let _ = sender.send(CmdResult::Output(output.to_string()));
        });

        Ok(receiver)
    }
}
// crossbeam/bbm/adhoc:1 ends here
