use crate::common::*;

use tokio::prelude::*;
use tokio::process::Command;
use tokio::signal::ctrl_c;
use tokio::time::{delay_for, Duration};

/// Manage process session
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
        // setsid -w external-cmd
        let mut command = Command::new("setsid");
        command.arg("-w").arg(program).kill_on_drop(true);

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

    /// A wrapper of std spawn method for saving session id.
    fn spawn(&mut self) -> Result<tokio::process::Child> {
        let child = self.command.spawn()?;
        let pid = child.id();
        self.sid = Some(pid);
        debug!("spawn new session: {}", pid);
        Ok(child)
    }

    /// send signal to child processes
    fn signal(&mut self, sig: &str) -> Result<()> {
        if let Some(sid) = self.sid {
            crate::process::signal_processes_by_session_id(sid, sig)?;
        }
        Ok(())
    }
}

impl Session {
    pub async fn start(&mut self) {
        let mut child = self.spawn().unwrap();

        // running timeout for 2 days
        let default_timeout = 3600 * 2;
        let mut timeout = delay_for(Duration::from_secs(self.timeout.unwrap_or(default_timeout)));
        // user interruption
        let mut ctrl_c = tokio::signal::ctrl_c();

        let v: usize = loop {
            tokio::select! {
                _ = &mut timeout => {
                    warn!("operation timed out");
                    break 1;
                }
                _ = ctrl_c => {
                    warn!("user interruption");
                    break 1;
                }
                _ = &mut child => {
                    info!("operation completed");
                    break 0;
                }
            }
        };

        if v == 1 {
            info!("Force to kill {}", child.id());
            self.kill().unwrap();
        } else {
            info!("checking orphaned processes ...");
            self.kill().unwrap();
        }
    }
}

impl Session {
    /// Run command with session manager.
    pub fn run(&mut self) -> Result<()> {
        let mut rt = tokio::runtime::Runtime::new().context("tokio runtime failure")?;
        rt.block_on(self.start());

        Ok(())
    }
}

use structopt::*;

/// A local runner that can make graceful exit
#[derive(StructOpt, Debug, Default)]
struct Runner {
    /// The program to be run.
    #[structopt(name = "program")]
    program: String,

    /// Job timeout in seconds
    #[structopt(long = "timeout", short = "t")]
    timeout: Option<u64>,

    /// Arguments that will be passed to `program`
    #[structopt(raw = true)]
    rest: Vec<String>,
}

pub fn enter_main() {
    gut::cli::setup_logger();
    let args = Runner::from_args();

    let mut session = Session::new(&args.program).timeout(args.timeout.unwrap_or(50));
    session.run().unwrap();
}

#[test]
fn test_tokio() -> Result<()> {
    let mut session = Session::new("sleep").arg("10").timeout(2);
    session.run()?;

    Ok(())
}
