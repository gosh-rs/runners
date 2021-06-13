// [[file:../runners.note::*imports][imports:1]]
//! For handling running task/job

use crate::common::*;

use serde::{Deserialize, Serialize};
use tempfile::{tempdir, tempdir_in, TempDir};
// imports:1 ends here

// [[file:../runners.note::*status][status:1]]
#[derive(Clone, Debug)]
enum Status {
    NotStarted,
    Running,
    /// failure code
    Failure(i32),
    Success,
}

impl Default for Status {
    fn default() -> Self {
        Self::NotStarted
    }
}

pub type Id = usize;
// status:1 ends here

// [[file:../runners.note::*base][base:1]]
/// Represents a computational job.
#[derive(Debug, Deserialize, Serialize)]
pub struct Job {
    // FIXME:
    pub(crate) input: String,
    pub(crate) script: String,

    #[serde(skip)]
    status: Status,

    // FIXME:
    // /// A short string describing the computation job.
    // name: String,
    /// Path to a file for saving input stream of computation
    inp_file: PathBuf,

    /// Path to a file for saving output stream of computation.
    out_file: PathBuf,

    /// Path to a file for saving error stream of computation.
    err_file: PathBuf,

    /// Path to a script file that defining how to start computation
    run_file: PathBuf,

    /// The working directory of computation
    #[serde(skip)]
    pub(crate) wrk_dir: Option<TempDir>,

    // command session
    #[serde(skip)]
    pub(crate) session: Option<tokio::process::Child>,

    /// Extra files required for computation
    extra_files: Vec<PathBuf>,
}

impl Job {
    ///
    /// Construct a Job with shell script of job run_file.
    ///
    /// # Parameters
    ///
    /// * script: the content of the script for running the job.
    ///
    pub fn new(script: &str) -> Self {
        Self {
            script: script.into(),
            input: String::new(),

            out_file: "job.out".into(),
            err_file: "job.err".into(),
            run_file: "run".into(),
            inp_file: "job.inp".into(),

            // state variables
            status: Status::default(),
            session: None,
            wrk_dir: None,
            extra_files: vec![],
        }
    }

    /// Set content of job stdin stream.
    fn with_stdin(mut self, content: &str) -> Self {
        self.input = content.into();
        self
    }

    pub fn wrk_dir(&self) -> &Path {
        if let Some(d) = &self.wrk_dir {
            d.path()
        } else {
            panic!("no working dir!")
        }
    }

    /// Return full path to computation input file (stdin).
    pub fn inp_file(&self) -> PathBuf {
        self.wrk_dir().join(&self.inp_file)
    }

    /// Return full path to computation output file (stdout).
    pub fn out_file(&self) -> PathBuf {
        self.wrk_dir().join(&self.out_file)
    }

    /// Return full path to computation error file (stderr).
    pub fn err_file(&self) -> PathBuf {
        self.wrk_dir().join(&self.err_file)
    }

    pub fn run_file(&self) -> PathBuf {
        self.wrk_dir().join(&self.run_file)
    }

    /// Return a list of full path to extra files required for computation.
    pub fn extra_files(&self) -> Vec<PathBuf> {
        self.extra_files.iter().map(|f| self.wrk_dir().join(f)).collect()
    }

    /// Check if job has been done correctly.
    pub fn is_done(&self) -> bool {
        let inpfile = self.inp_file();
        let outfile = self.out_file();
        let errfile = self.err_file();

        if self.wrk_dir().is_dir() {
            if outfile.is_file() && inpfile.is_file() {
                if let Ok(time2) = outfile.metadata().and_then(|m| m.modified()) {
                    if let Ok(time1) = inpfile.metadata().and_then(|m| m.modified()) {
                        if time2 >= time1 {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    /// Update file timestamps to make sure `is_done` call return true.
    pub fn fake_done(&self) {
        unimplemented!()
    }

    /// Add a new file into extra-files list.
    pub fn attach_file<P: AsRef<Path>>(&mut self, file: P) {
        let file: PathBuf = file.as_ref().into();
        if !self.extra_files.contains(&file) {
            self.extra_files.push(file);
        } else {
            warn!("try to attach a dumplicated file: {}!", file.display());
        }
    }
}
// base:1 ends here

// [[file:../runners.note::*core][core:1]]
use tokio::io::AsyncWriteExt;

impl Job {
    /// Create runnable script file and stdin file from self.script and
    /// self.input.
    pub fn build(&mut self) {
        use std::fs::File;
        use std::os::unix::fs::OpenOptionsExt;

        // create working directory in scratch space.
        // let wdir = tempfile::tempdir().expect("temp dir");
        let wdir = tempfile::TempDir::new_in(".").expect("temp dir");
        self.wrk_dir = Some(wdir);

        // create run file
        let file = self.run_file();

        // make run script executable
        match std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .mode(0o770)
            .open(&file)
        {
            Ok(mut f) => {
                let _ = f.write_all(self.script.as_bytes());
                trace!("script content wrote to: {}.", file.display());
            }
            Err(e) => {
                panic!("Error whiling creating job run file: {}", e);
            }
        }
        let file = self.inp_file();
        match File::create(&self.inp_file()) {
            Ok(mut f) => {
                let _ = f.write_all(self.input.as_bytes());
                trace!("input content wrote to: {}.", file.display());
            }
            Err(e) => {
                panic!("Error while creating job input file: {}", e);
            }
        }
    }

    /// Wait for background command to complete.
    pub async fn wait(&mut self) {
        if let Some(mut child) = self.session.take() {
            child.wait_with_output().await;
        } else {
            error!("Job not started yet.");
        }
    }

    /// Terminate background command session.
    fn terminate(&mut self) {
        if let Some(child) = &mut self.session {
            if let Some(sid) = child.id() {
                crate::process::signal_processes_by_session_id(sid, "SIGTERM").expect("term session");
                info!("Job with command session {} has been terminated.", sid);
            }
        } else {
            debug!("Job not started yet.");
        }
    }

    /// Run command in background.
    pub async fn start(&mut self) -> Result<()> {
        let wdir = self.wrk_dir();
        info!("job work direcotry: {}", wdir.display());

        let mut child = tokio::process::Command::new(&self.run_file())
            .current_dir(wdir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn command session");

        let mut stdin = child.stdin.take().expect("child did not have a handle to stdout");
        let mut stdout = child.stdout.take().expect("child did not have a handle to stdout");
        let mut stderr = child.stderr.take().expect("child did not have a handle to stderr");

        // NOTE: suppose stdin stream is small.
        stdin.write_all(self.input.as_bytes()).await;

        // redirect stdout and stderr to files for user inspection.
        let mut fout = tokio::fs::File::create(self.out_file()).await?;
        let mut ferr = tokio::fs::File::create(self.err_file()).await?;
        tokio::io::copy(&mut stdout, &mut fout).await?;
        tokio::io::copy(&mut stderr, &mut ferr).await?;

        let sid = child.id();
        info!("command running in session {:?}", sid);
        self.session = Some(child);

        Ok(())
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.terminate();
    }
}
// core:1 ends here
