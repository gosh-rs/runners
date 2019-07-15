// imports

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*imports][imports:1]]
use std::path::{Path, PathBuf};

use tempfile::{tempdir, tempdir_in, TempDir};

use crate::common::*;
// imports:1 ends here

// job

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*job][job:1]]
/// Represents a computational job.
#[derive(Debug)]
pub struct Job {
    /// A short string describing the computation job.
    name: String,

    /// Path to a file for saving input stream of computation
    inp_file: PathBuf,

    /// Path to a file for saving output stream of computation.
    out_file: PathBuf,

    /// Path to a file for saving error stream of computation.
    err_file: PathBuf,

    /// Path to a script file that defining how to start computation
    run_file: PathBuf,

    /// The working directory of computation
    wrk_dir: TempDir,

    /// Extra files required for computation
    extra_files: Vec<PathBuf>,
}

impl Default for Job {
    fn default() -> Self {
        Self {
            name: "default-job".into(),
            run_file: "run.sh".into(),
            inp_file: "input".into(),
            out_file: "output".into(),
            err_file: "debug".into(),
            wrk_dir: tempfile::tempdir().expect("temp directory"),
            extra_files: vec![],
        }
    }
}

impl Job {
    /// Construct a job with a name.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn wrk_dir(&self) -> &Path {
        self.wrk_dir.path()
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
        self.extra_files
            .iter()
            .map(|f| self.wrk_dir().join(f))
            .collect()
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

fn abs_path(dir: &PathBuf, file: &PathBuf) -> PathBuf {
    dir.join(file)
}
// job:1 ends here

// worker

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*worker][worker:1]]
pub struct Worker {
    job: Job,
}

impl Worker {
    /// Run the job on the remote console and download the data backgoundly when
    /// the job done.
    pub fn run(&mut self, job: &Job) -> Result<()> {
        unimplemented!()
    }
}
// worker:1 ends here

// kill

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*kill][kill:1]]

// kill:1 ends here

// test

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*test][test:1]]
#[test]
fn test_job() {
    let mut job = Job::new("test");
    dbg!(job.wrk_dir());
    dbg!(job.err_file());
    dbg!(job.run_file());
    dbg!(job.inp_file());
    dbg!(job.out_file());
    dbg!(job.is_done());

    job.attach_file("/tmp/a.xyz");
    dbg!(job.extra_files());
}
// test:1 ends here