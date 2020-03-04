// [[file:~/Workspace/Programming/gosh-rs/runner/runners.note::*imports][imports:1]]
use crate::common::*;
// imports:1 ends here

// [[file:~/Workspace/Programming/gosh-rs/runner/runners.note::*signal][signal:1]]
use nix::sys::signal::Signal;
#[test]
#[ignore]
fn test_unix_signal() {
    let s: Signal = "SIGINT".parse().unwrap();
    dbg!(s);
}
// signal:1 ends here

// [[file:~/Workspace/Programming/gosh-rs/runner/runners.note::*timestamp][timestamp:1]]
use chrono::*;

/// Convert unix timestamp in floating point seconds to `DateTime`
fn float_unix_timestamp_to_date_time(t: f64) -> DateTime<Utc> {
    let nano = t.fract() * 1_000_000_000f64;
    Utc.timestamp(t.trunc() as i64, nano.round() as u32)
}
// timestamp:1 ends here

// [[file:~/Workspace/Programming/gosh-rs/runner/runners.note::*unique process][unique process:1]]
use chrono::*;

#[derive(Clone, PartialEq, Eq, Hash, Copy, Debug)]
pub(crate) struct UniqueProcessId(i32, DateTime<Utc>);

impl UniqueProcessId {
    /// construct from pid. return error if the process `pid` not alive.
    fn from_pid(pid: i32) -> Result<Self> {
        if let Ok(p) = psutil::process::Process::new(pid) {
            if p.is_alive() {
                return Ok(Self::from_process(p));
            }
        }
        bail!("invalid pid: {}", pid)
    }

    /// construct from psutil `Process` struct (1.x branch only)
    fn from_process(p: psutil::process::Process) -> Self {
        let dt = float_unix_timestamp_to_date_time(p.starttime);
        Self(p.pid, dt)
    }

    /// for psutil
    pub fn pid(&self) -> i32 {
        self.0
    }
}
// unique process:1 ends here

// [[file:~/Workspace/Programming/gosh-rs/runner/runners.note::*impl/psutil][impl/psutil:1]]
use std::collections::HashSet;

/// Find child processes using psutil (without using shell commands)
///
/// # Reference
///
/// https://github.com/borntyping/rust-psutil/blob/master/examples/ps.rs
fn impl_get_child_processes_by_session_id(sid: u32) -> Result<HashSet<UniqueProcessId>> {
    let processes = psutil::process::all().context("psutil all processes")?;

    // collect pids then kill
    let child_processes = processes
        .into_iter()
        .filter_map(|p| {
            if p.session == sid as i32 {
                Some(UniqueProcessId::from_process(p))
            } else {
                None
            }
        })
        .collect();

    Ok(child_processes)
}

/// Signal child processes by session id
///
/// Note: currently, psutil has no API for kill with signal other than SIGKILL
///
fn impl_signal_processes_by_session_id(sid: u32, signal: &str) -> Result<()> {
    let signal = signal
        .parse::<Signal>()
        .with_context(|| format!("invalid signal str: {}", signal))?;

    let child_processes = get_child_processes_by_session_id(sid)?;
    info!(
        "session {} has {} child processes",
        sid,
        child_processes.len()
    );

    for child in child_processes {
        debug!("{:?}", child);

        // refresh process id from /proc before kill
        // check starttime to avoid re-used pid
        let pid = child.pid();
        if let Ok(process) = UniqueProcessId::from_pid(pid) {
            if process == child {
                nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), signal)?;
                debug!("process {} was killed", pid);
            } else {
                warn!("process id {} was reused?", pid);
            }
        } else {
            info!("process {} already terminated.", pid);
        }
    }

    Ok(())
}
// impl/psutil:1 ends here

// [[file:~/Workspace/Programming/gosh-rs/runner/runners.note::*pub][pub:1]]
/// Signal all child processes in session `sid`
pub(crate) fn signal_processes_by_session_id(sid: u32, signal: &str) -> Result<()> {
    info!("killing session {} with signal {}", sid, signal);
    impl_signal_processes_by_session_id(sid, signal)
}

/// Find child processes in session `sid`
///
/// # Reference
///
/// https://github.com/borntyping/rust-psutil/blob/master/examples/ps.rs
pub(crate) fn get_child_processes_by_session_id(sid: u32) -> Result<HashSet<UniqueProcessId>> {
    impl_get_child_processes_by_session_id(sid)
}
// pub:1 ends here
