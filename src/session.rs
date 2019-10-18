// imports

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*imports][imports:1]]
use crate::common::*;
// imports:1 ends here

// base

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*base][base:1]]
use std::os::unix::process::CommandExt;
use std::process::Command;

pub(crate) struct Session {
    /// Session ID
    sid: u32,
    /// Child process. The process might be removed prematurely, in which case we do not kill
    /// anything
    child: std::process::Child,
}

impl Session {
    /// Create a new session.
    ///
    /// # unsafe
    ///
    /// It is unsafe to put any arbitrary child process into a process guard,
    /// mainly because the guard relies on the child not having been waited on
    /// beforehand. Otherwise, it cannot be guaranteed that the child process
    /// has not exited and its PID been reused, potentially killing an innocent
    /// bystander process on `Drop`.
    pub unsafe fn new(mut cmd: Command) -> Self {
        let child = cmd
            // Don't check the error of setsid because it fails if we're the
            // process leader already. We just forked so it shouldn't return
            // error, but ignore it anyway.
            .pre_exec(|| {
                nix::unistd::setsid().ok();
                Ok(())
            })
            .spawn()
            .unwrap();

        let sid = child.id();
        Self { sid, child }
    }

    /// terminate child processes in a session.
    pub fn terminate(&self) -> Result<()> {
        signal_processes_by_session_id(self.sid, "SIGTERM")
    }

    /// Kill processes in a session.
    pub fn kill(&self) -> Result<()> {
        signal_processes_by_session_id(self.sid, "SIGKILL")
    }

    /// Resume processes in a session.
    pub fn resume(&self) -> Result<()> {
        signal_processes_by_session_id(self.sid, "SIGCONT")
    }

    /// Pause processes in a session.
    pub fn pause(&self) -> Result<()> {
        signal_processes_by_session_id(self.sid, "SIGSTOP")
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
use duct::cmd;

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

    let child_pids = get_child_processes_by_session_id(sid)?;
    for pid in child_pids {
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), signal)?;
    }

    Ok(())
}

/// Find child processes using psutil (without using shell commands)
///
/// # Reference
///
/// https://github.com/borntyping/rust-psutil/blob/master/examples/ps.rs
fn get_child_processes_by_session_id(sid: u32) -> Result<Vec<i32>> {
    if let Ok(processes) = psutil::process::all() {
        // collect pids then kill
        let child_processes: Vec<_> = processes
            .into_iter()
            .filter_map(|p| {
                if p.session == sid as i32 {
                    Some(p.session)
                } else {
                    None
                }
            })
            .inspect(|p| {
                dbg!(p);
            })
            .collect();

        Ok(child_processes)
    } else {
        Ok(vec![])
    }
}
// impl/psutil:1 ends here

// test

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*test][test:1]]
#[test]
fn test() {
    //
}
// test:1 ends here
