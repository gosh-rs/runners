// imports

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*imports][imports:1]]
use crate::common::*;
// imports:1 ends here

// base

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*base][base:1]]
use std::os::unix::process::CommandExt;
use std::process::Command;

pub struct Session {
    /// Session ID
    sid: Option<u32>,

    /// Arguments that will be passed to `program`
    rest: Vec<String>,

    /// Job timeout in seconds
    timeout: Option<u64>,

    command: Command,
}

impl Session {
    /// Create a new session.
    pub fn new(program: &str) -> Self {
        let mut command = Command::new("setsid");
        command.arg("-w").arg(program);
        Self {
            command,
            sid: None,
            timeout: None,
            rest: vec![],
        }
    }

    /// Set program argument
    pub fn arg<S: AsRef<str>>(mut self, arg: S) -> Self {
        self.command.arg(arg.as_ref());
        self
    }

    /// Return a mutable reference to internal `Command` struct.
    pub(crate) fn command(&mut self) -> &mut Command {
        &mut self.command
    }

    /// A wrapper of std spawn method for saving session id.
    pub fn spawn(&mut self) -> Result<std::process::Child> {
        let child = self.command.spawn()?;
        self.sid = Some(child.id());
        Ok(child)
    }

    /// Set program running timeout.
    pub fn timeout(mut self, t: u64) -> Self {
        self.timeout = Some(t);
        self
    }

    /// Terminate child processes in a session.
    pub fn terminate(&mut self) -> Result<()> {
        self.signal("SIGTERM")
    }

    /// Kill processes in a session.
    pub fn kill(&mut self) -> Result<()> {
        self.signal("SIGKILL")
    }

    /// Resume processes in a session.
    pub fn resume(&mut self) -> Result<()> {
        self.signal("SIGCONT")
    }

    /// Pause processes in a session.
    pub fn pause(&mut self) -> Result<()> {
        self.signal("SIGSTOP")
    }

    /// send signal to child processes
    fn signal(&mut self, sig: &str) -> Result<()> {
        if let Some(sid) = self.sid {
            signal_processes_by_session_id(sid, sig)?;
        }
        Ok(())
    }
}
// base:1 ends here

// drop

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*drop][drop:1]]
impl Drop for Session {
    #[inline]
    fn drop(&mut self) {
        self.kill();
    }
}
// drop:1 ends here

// impl/psutil
// impl based on psutil and nix crates.

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*impl/psutil][impl/psutil:1]]
/// Signal child processes by session id
///
/// Note: currently, psutil has no API for kill with signal other than SIGKILL
///
pub(crate) fn signal_processes_by_session_id(sid: u32, signal: &str) -> Result<()> {
    use nix::sys::signal::Signal;
    let signal = match signal {
        "SIGINT" => Signal::SIGINT,
        "SIGTERM" => Signal::SIGTERM,
        "SIGKILL" => Signal::SIGKILL,
        "SIGCONT" => Signal::SIGCONT,
        "SIGSTOP" => Signal::SIGSTOP,
        _ => unimplemented!(),
    };

    let child_processes = get_child_processes_by_session_id(sid)?;
    for UniqueProcessId(pid, ctime) in child_processes {
        // refresh process id from /proc before kill
        if let Ok(process) = psutil::process::Process::new(pid) {
            // check starttime to avoid re-used pid
            if process.starttime == ctime {
                nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), signal)?;
            }
        }
    }

    Ok(())
}

/// Find child processes using psutil (without using shell commands)
///
/// # Reference
///
/// https://github.com/borntyping/rust-psutil/blob/master/examples/ps.rs
fn get_child_processes_by_session_id(sid: u32) -> Result<Vec<UniqueProcessId>> {
    if let Ok(processes) = psutil::process::all() {
        // collect pids then kill
        let child_processes: Vec<_> = processes
            .into_iter()
            .filter_map(|p| {
                if p.session == sid as i32 {
                    Some(UniqueProcessId(p.session, p.starttime))
                } else {
                    None
                }
            })
            .collect();

        Ok(child_processes)
    } else {
        Ok(vec![])
    }
}

struct UniqueProcessId(i32, f64);
// impl/psutil:1 ends here

// test

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*test][test:1]]

// test:1 ends here
