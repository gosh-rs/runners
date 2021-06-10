// [[file:../runners.note::*imports][imports:1]]
use crate::common::*;
// imports:1 ends here

// [[file:../runners.note::*signal][signal:1]]
use nix::sys::signal::Signal;
#[test]
#[ignore]
fn test_unix_signal() {
    let s: Signal = "SIGINT".parse().unwrap();
    dbg!(s);
}
// signal:1 ends here

// [[file:../runners.note::*timestamp][timestamp:1]]
use chrono::*;

/// Convert unix timestamp in floating point seconds to `DateTime`
fn float_unix_timestamp_to_date_time(t: f64) -> DateTime<Utc> {
    let nano = t.fract() * 1_000_000_000f64;
    Utc.timestamp(t.trunc() as i64, nano.round() as u32)
}
// timestamp:1 ends here

// [[file:../runners.note::*unique process][unique process:1]]
use std::collections::HashSet;
use std::time::Duration;

#[derive(Clone, PartialEq, Eq, Hash, Copy, Debug)]
pub(crate) struct UniqueProcessId(u32, Duration);

impl UniqueProcessId {
    /// construct from pid. return error if the process `pid` not alive.
    fn from_pid(pid: u32) -> Result<Self> {
        if let Ok(p) = psutil::process::Process::new(pid) {
            if p.is_running() {
                return Ok(Self::from_process(p));
            }
        }
        bail!("invalid pid: {}", pid)
    }

    /// construct from psutil `Process` struct (1.x branch only)
    fn from_process(p: psutil::process::Process) -> Self {
        Self(p.pid(), p.create_time())
    }

    /// Process Id
    pub fn pid(&self) -> u32 {
        self.0
    }
}
// unique process:1 ends here

// [[file:../runners.note::*impl/psutil/v3][impl/psutil/v3:1]]
/// Find child processes using psutil (without using shell commands)
///
/// # Reference
///
/// https://github.com/borntyping/rust-psutil/blob/master/examples/ps.rs
fn get_child_processes_by_session_id(sid: u32) -> Result<HashSet<UniqueProcessId>> {
    // for Process::procfs_stat method
    use psutil::process::os::linux::ProcessExt;

    let child_processes = psutil::process::pids()?
        .into_iter()
        .filter_map(|pid| psutil::process::Process::new(pid).ok())
        .filter_map(|p| p.procfs_stat().ok().map(|s| (p, s)))
        .filter_map(|(p, s)| {
            if s.session as u32 == sid {
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
    let signal: Signal = signal
        .parse()
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
                nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), signal)?;
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
// impl/psutil/v3:1 ends here

// [[file:../runners.note::*pub][pub:1]]
/// Signal all child processes in session `sid`
pub fn signal_processes_by_session_id(sid: u32, signal: &str) -> Result<()> {
    info!("killing session {} with signal {}", sid, signal);
    impl_signal_processes_by_session_id(sid, signal)
}
// pub:1 ends here
