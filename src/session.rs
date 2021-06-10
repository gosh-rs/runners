// [[file:../runners.note::*imports][imports:1]]
use crate::common::*;

use tokio::prelude::*;
use tokio::process::Command;
use tokio::signal::ctrl_c;
use tokio::time::{delay_for, Duration};
// imports:1 ends here

// [[file:../runners.note::*base][base:1]]
/// Manage process session
#[derive(Debug)]
pub struct Session {
    /// Session ID
    sid: Option<u32>,

    /// Arguments that will be passed to `program`
    rest: Vec<String>,

    /// Job timeout in seconds
    timeout: Option<u32>,

    /// The external command
    command: Command,

    /// Stdin input bytes
    stdin_bytes: Vec<u8>,

    cmd_output: Option<std::process::Output>,
}

impl Session {
    /// Create a new session.
    pub fn new(program: &str) -> Self {
        // setsid -w external-cmd
        let mut command = Command::new("setsid");
        // do not kill command when `Child` drop
        command.arg("-w").arg(program).kill_on_drop(false);

        Self {
            command,
            sid: None,
            timeout: None,
            rest: vec![],
            stdin_bytes: vec![],
            cmd_output: None,
        }
    }

    /// Adds multiple arguments to pass to the program.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.command.args(args);
        self
    }

    /// Set program argument
    pub fn arg<S: AsRef<str>>(mut self, arg: S) -> Self {
        self.command.arg(arg.as_ref());
        self
    }

    /// Sets the working directory for the child process.
    pub fn dir<P: AsRef<std::path::Path>>(mut self, dir: P) -> Self {
        // FIXME: use absolute path?
        self.command.current_dir(dir);
        self
    }

    /// Inserts or updates an environment variable mapping.
    pub fn env<K, V>(mut self, key: K, val: V) -> Self
    where
        K: AsRef<std::ffi::OsStr>,
        V: AsRef<std::ffi::OsStr>,
    {
        self.command.env(key, val);
        self
    }

    /// Set program running timeout.
    pub fn timeout(mut self, t: u32) -> Self {
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

    /// Use bytes or a string as stdin
    /// A worker thread will write the input at runtime.
    pub fn stdin_bytes<T: Into<Vec<u8>>>(mut self, bytes: T) -> Self {
        self.stdin_bytes = bytes.into();
        self
    }

    /// send signal to child processes
    pub fn signal(&mut self, sig: &str) -> Result<()> {
        if let Some(sid) = self.sid {
            // let out = duct::cmd!("pstree", "-p", format!("{}", sid)).read()?;
            // dbg!(out);
            crate::process::signal_processes_by_session_id(sid, sig)?;
        } else {
            debug!("process not started yet");
        }
        Ok(())
    }
}
// base:1 ends here

// [[file:../runners.note::*core][core:1]]
impl Session {
    async fn start(&mut self) -> Result<()> {
        use std::process::Stdio;

        // pipe stdin_bytes to program's stdin
        let mut child = self
            .command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        self.sid = Some(child.id());

        child
            .stdin
            .take()
            .context("child did not have a handle to stdin")?
            .write_all(&self.stdin_bytes)
            .await
            .context("Failed to write to stdin")?;

        let cmd_output = child.wait_with_output();

        // running timeout for 2 days
        let default_timeout = 3600 * 2;
        let timeout = delay_for(Duration::from_secs(
            self.timeout.unwrap_or(default_timeout) as u64
        ));
        // user interruption
        let ctrl_c = tokio::signal::ctrl_c();

        let v: usize = loop {
            tokio::select! {
                _ = timeout => {
                    eprintln!("Program timed out");
                    break 1;
                }
                _ = ctrl_c => {
                    eprintln!("User interruption");
                    break 1;
                }
                o = cmd_output => {
                    println!("Program completed");
                    match o {
                        Ok(o) => {
                            self.cmd_output = Some(o);
                        }
                        Err(e) => {
                            error!("cmd error: {:?}", e);
                        }
                    }
                    break 0;
                }
            }
        };

        if v == 1 {
            info!("program was interrupted.");
            self.kill()?;
        } else {
            info!("checking orphaned processes ...");
            self.kill()?;
        }

        Ok(())
    }
}
// core:1 ends here

// [[file:../runners.note::*pub][pub:1]]
impl Session {
    /// Run command with session manager.
    pub fn run(mut self) -> Result<std::process::Output> {
        let mut rt = tokio::runtime::Runtime::new().context("tokio runtime failure")?;
        rt.block_on(self.start())?;

        self.cmd_output.take().ok_or(format_err!("no cmd output"))
    }
}
// pub:1 ends here

// [[file:../runners.note::*cli][cli:1]]
use gut::cli::*;

/// A local runner that can make graceful exit
#[derive(StructOpt, Debug, Default)]
struct Runner {
    /// Job timeout in seconds. The default timeout is 30 days.
    #[structopt(long = "timeout", short = "t")]
    timeout: Option<u32>,

    #[structopt(flatten)]
    verbose: gut::cli::Verbosity,

    /// Command line to call a program
    #[structopt(raw = true, required = true)]
    cmdline: Vec<String>,
}

pub fn enter_main() -> Result<()> {
    let args = Runner::from_args();
    args.verbose.setup_logger();

    let program = &args.cmdline[0];
    let rest = &args.cmdline[1..];

    let o = Session::new(program)
        .args(rest)
        .timeout(args.timeout.unwrap_or(3600 * 24 * 30))
        .run()?;
    dbg!(o);

    Ok(())
}
// cli:1 ends here

// [[file:../runners.note::*test][test:1]]
#[test]
fn test_tokio() -> Result<()> {
    gut::cli::setup_logger_for_test();

    let mut session = Session::new("sleep").arg("10").timeout(1);
    session.run().ok();

    Ok(())
}
// test:1 ends here
