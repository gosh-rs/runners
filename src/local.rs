// [[file:../runners.note::*imports][imports:1]]
use crate::common::*;

use tokio::process::Command;
use tokio::signal::ctrl_c;
use tokio::time::Duration;
// imports:1 ends here

// [[file:../runners.note::*base][base:1]]
/// Manage process group using session
#[derive(Debug)]
struct Session {
    /// Session ID
    sid: Option<u32>,

    /// Arguments that will be passed to `program`
    rest: Vec<String>,

    /// Job timeout in seconds
    timeout: Option<u32>,

    /// The external command
    command: Command,
}

impl Session {
    /// Create a new session.
    pub fn new(program: &str) -> Self {
        use crate::process::ProcessGroupExt;

        // setsid -w external-cmd
        let mut command = Command::new(program);
        // do not kill command when `Child` drop
        command.kill_on_drop(false).new_process_group();

        Self {
            command,
            sid: None,
            timeout: None,
            rest: vec![],
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
        let mut child = self.command.spawn()?;
        self.sid = child.id();
        // Ensure we close any stdio handles so we can't deadlock
        // waiting on the child which may be waiting to read/write
        // to a pipe we're holding.
        child.stdin.take();
        child.stdout.take();
        child.stderr.take();

        // running timeout for 2 days
        let default_timeout = 3600 * 2;
        let timeout = tokio::time::sleep(Duration::from_secs(self.timeout.unwrap_or(default_timeout) as u64));
        tokio::pin!(timeout);
        // user interruption
        let ctrl_c = tokio::signal::ctrl_c();

        let v: usize = loop {
            tokio::select! {
                _ = &mut timeout => {
                    eprintln!("program timed out");
                    break 1;
                }
                _ = ctrl_c => {
                    eprintln!("user interruption");
                    break 1;
                }
                o = child.wait() => {
                    println!("program completed");
                    match o {
                        Ok(o) => {
                            dbg!(o);
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
    
    /// Run command with session manager.
    pub fn run(mut self) -> Result<()> {
        let mut rt = tokio::runtime::Runtime::new().context("tokio runtime failure")?;
        rt.block_on(self.start())?;

        Ok(())
    }
}
// core:1 ends here

// [[file:../runners.note::*cli][cli:1]]
use gut::cli::*;

/// A local runner that can make graceful exit
#[derive(StructOpt, Debug, Default)]
struct RunnerCli {
    #[structopt(flatten)]
    verbose: gut::cli::Verbosity,

    /// Job timeout in seconds. The default timeout is 30 days.
    #[structopt(long = "timeout", short = "t")]
    timeout: Option<u32>,

    /// Command line to call a program
    #[structopt(raw = true, required = true)]
    cmdline: Vec<String>,
}

impl RunnerCli {
    fn enter_main<I>(iter: I) -> Result<()>
    where
        Self: Sized,
        I: IntoIterator,
        I::Item: Into<std::ffi::OsString> + Clone,
    {
        let args = RunnerCli::from_iter_safe(iter)?;
        args.verbose.setup_logger();

        let program = &args.cmdline[0];
        let rest = &args.cmdline[1..];

        Session::new(program)
            .args(rest)
            .timeout(args.timeout.unwrap_or(3600 * 24 * 30))
            .run()?;

        Ok(())
    }
}

pub fn enter_main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    assert!(args.len() >= 1, "{:?}", args);
    // The path to symlink file that invoking the real program
    let invoke_path: &Path = &args[0].as_ref();

    // check file extension for sure (should be foo.run)
    // REVIEW: must be carefully here: not to enter infinite loop
    if let Some("run") = invoke_path.extension().and_then(|s| s.to_str()) {
        // apply symlink magic
        // call the program that symlink pointing to
        let invoke_exe = invoke_path.file_stem().context("invoke exe name")?;

        // The path to real executable binary
        let real_path = std::env::current_exe().context("Failed to get exe path")?;
        println!("Runner exe path: {:?}", real_path);
        let real_exe = real_path.file_name().context("real exe name")?;

        if real_exe != invoke_exe {
            let runner_args = [&real_exe.to_string_lossy(), "-v", "--", &invoke_exe.to_string_lossy()];

            let cmdline: Vec<_> = runner_args
                .iter()
                .map(|s| s.to_string())
                .chain(args.iter().cloned().skip(1))
                .collect();
            println!("runner will call {:?} with {:?}", invoke_exe, cmdline.join(" "));
            return RunnerCli::enter_main(cmdline);
        }
    }
    // run in a normal way
    RunnerCli::enter_main(std::env::args())
}
// cli:1 ends here
