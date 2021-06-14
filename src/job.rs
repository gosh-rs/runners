// [[file:../runners.note::*imports][imports:1]]
//! For handling running task/job

use crate::common::*;

use serde::{Deserialize, Serialize};
use tempfile::{tempdir, tempdir_in, TempDir};
// imports:1 ends here

// [[file:../runners.note::*base][base:1]]
/// Represents a computational job.
#[derive(Debug, Deserialize, Serialize)]
pub struct Job {
    // FIXME:
    pub(crate) input: String,
    pub(crate) script: String,

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
// core:1 ends here

// [[file:../runners.note::*drop][drop:1]]
impl Drop for Job {
    fn drop(&mut self) {
        self.terminate();
    }
}
// drop:1 ends here

// [[file:../runners.note::*core][core:1]]
mod db {
    use super::*;

    use bytes::Bytes;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    pub use super::impl_jobs_slotmap::Id;
    use super::impl_jobs_slotmap::JobKey;
    use super::impl_jobs_slotmap::Jobs;

    // pub use super::impl_jobs_slab::Id;
    // use super::impl_jobs_slab::JobKey;
    // use super::impl_jobs_slab::Jobs;

    /// A simple in-memory DB for computational jobs.
    #[derive(Clone)]
    pub struct Db {
        inner: Arc<Mutex<Jobs>>,
    }

    impl Db {
        /// Create an empty `Db`
        pub fn new() -> Self {
            Self {
                inner: Arc::new(Mutex::new(Jobs::new())),
            }
        }

        /// Update the job in `id` with a `new_job`.
        pub async fn update_job(&mut self, id: JobId, new_job: Job) -> Result<()> {
            debug!("update_job: id={}, job={:?}", id, new_job);
            let mut jobs = self.inner.lock().await;
            let k = jobs.check_job(id)?;
            jobs[k] = new_job;

            Ok(())
        }

        pub async fn delete_job(&mut self, id: JobId) -> Result<()> {
            info!("delete_job: id={}", id);
            self.inner.lock().await.remove(id)?;
            Ok(())
        }

        pub async fn get_job_list(&self) -> Vec<JobId> {
            self.inner.lock().await.iter().map(|(k, _)| k).collect()
        }

        pub async fn clear_jobs(&mut self) {
            self.inner.lock().await.clear();
        }

        pub async fn wait_job(&self, id: JobId) -> Result<()> {
            info!("wait_job: id={}", id);
            let mut jobs = self.inner.lock().await;
            let k = jobs.check_job(id)?;
            jobs[k].start().await?;
            jobs[k].wait().await;
            Ok(())
        }

        pub async fn put_job_file(&mut self, id: JobId, file: String, body: Bytes) -> Result<()> {
            debug!("put_job_file: id={}", id);

            let jobs = self.inner.lock().await;
            let id = jobs.check_job(id)?;

            let job = &jobs[id];
            let p = job.wrk_dir().join(&file);
            info!("client request to put a file: {}", p.display());
            match std::fs::File::create(p) {
                Ok(mut f) => {
                    f.write_all(&body).context("write job file")?;
                    Ok(())
                }
                Err(e) => {
                    bail!("create file error:\n{}", e);
                }
            }
        }

        /// Insert job into the queue.
        pub async fn insert_job(&mut self, mut job: Job) -> JobId {
            info!("create_job: {:?}", job);
            let mut jobs = self.inner.lock().await;
            job.build();

            let jid = jobs.insert(job);
            info!("Job {} created.", jid);
            jid
        }

        pub async fn get_job_file(&self, id: JobId, file: String) -> Result<Vec<u8>> {
            debug!("get_job_file: id={}", id);
            let jobs = self.inner.lock().await;
            let id = jobs.check_job(id)?;
            let job = &jobs[id];
            let p = job.wrk_dir().join(&file);
            info!("client request file: {}", p.display());

            let mut buffer = Vec::new();
            let _ = std::fs::File::open(p)
                .context("open file")?
                .read_to_end(&mut buffer)
                .context("read file")?;
            Ok(buffer)
        }

        pub async fn list_job_files(&self, id: JobId) -> Result<Vec<PathBuf>> {
            info!("list files for job {}", id);
            let jobs = self.inner.lock().await;
            let id = jobs.check_job(id)?;

            let mut list = vec![];
            let job = &jobs[id];
            for entry in std::fs::read_dir(job.wrk_dir()).context("list dir")? {
                if let Ok(entry) = entry {
                    let p = entry.path();
                    if p.is_file() {
                        list.push(p);
                    }
                }
            }
            Ok(list)
        }
    }
}
// core:1 ends here

// [[file:../runners.note::*slab][slab:1]]
mod impl_jobs_slab {
    use super::*;
    use slab::Slab;

    pub type Id = usize;
    pub(super) type JobKey = usize;

    pub struct Jobs {
        inner: Slab<Job>,
    }

    impl Jobs {
        pub fn new() -> Self {
            Self { inner: Slab::new() }
        }

        // Look for the specified Job...
        pub fn check_job(&self, id: Id) -> Result<JobKey> {
            if self.inner.contains(id) {
                Ok(id)
            } else {
                bail!("Job id not found: {}", id);
            }
        }

        pub fn insert(&mut self, job: Job) -> Id {
            self.inner.insert(job)
        }

        pub fn remove(&mut self, id: JobKey) -> Result<()> {
            let _ = self.inner.remove(id);
            Ok(())
        }

        pub fn clear(&mut self) {
            self.inner.clear();
        }

        pub fn iter(&self) -> impl Iterator<Item = (Id, &Job)> {
            self.inner.iter()
        }
    }

    impl std::ops::Index<JobKey> for Jobs {
        type Output = Job;

        fn index(&self, key: JobKey) -> &Self::Output {
            &self.inner[key]
        }
    }

    impl std::ops::IndexMut<JobKey> for Jobs {
        fn index_mut(&mut self, key: JobKey) -> &mut Self::Output {
            &mut self.inner[key]
        }
    }
}
// slab:1 ends here

// [[file:../runners.note::*slotmap][slotmap:1]]
mod impl_jobs_slotmap {
    use super::*;

    use bimap::BiMap;
    use slotmap::Key;
    use slotmap::{DefaultKey, SlotMap};

    pub type Id = usize;
    pub(super) type JobKey = DefaultKey;

    pub struct Jobs {
        inner: SlotMap<DefaultKey, Job>,
        mapping: BiMap<usize, JobKey>,
    }

    impl Jobs {
        /// Create empty `Jobs`
        pub fn new() -> Self {
            Self {
                inner: SlotMap::new(),
                mapping: BiMap::new(),
            }
        }

        /// Look for the Job with `id`, returning error if the job with `id`
        /// does not exist.
        pub fn check_job(&self, id: Id) -> Result<JobKey> {
            if let Some(&k) = self.mapping.get_by_left(&id) {
                Ok(k)
            } else {
                bail!("Job id not found: {}", id);
            }
        }

        /// Insert a new Job into database, returning Id for later operations.
        pub fn insert(&mut self, job: Job) -> Id {
            let k = self.inner.insert(job);
            let n = self.mapping.len() + 1;
            if let Err(e) = self.mapping.insert_no_overwrite(n, k) {
                panic!("invalid {:?}", e);
            }
            n
        }

        /// Remove the job with `id`
        pub fn remove(&mut self, id: Id) -> Result<()> {
            let k = self.check_job(id)?;
            let _ = self.inner.remove(k);
            Ok(())
        }

        /// Remove all created jobs
        pub fn clear(&mut self) {
            self.inner.clear();
        }

        /// Iterator over a tuple of `Id` and `Job`.
        pub fn iter(&self) -> impl Iterator<Item = (Id, &Job)> {
            self.inner.iter().map(move |(k, v)| (self.to_id(k), v))
        }

        fn to_id(&self, k: JobKey) -> Id {
            if let Some(&id) = self.mapping.get_by_right(&k) {
                id
            } else {
                panic!("invalid job key {:?}", k);
            }
        }
    }

    impl std::ops::Index<JobKey> for Jobs {
        type Output = Job;

        fn index(&self, key: JobKey) -> &Self::Output {
            &self.inner[key]
        }
    }

    impl std::ops::IndexMut<JobKey> for Jobs {
        fn index_mut(&mut self, key: JobKey) -> &mut Self::Output {
            &mut self.inner[key]
        }
    }
}
// slotmap:1 ends here

// [[file:../runners.note::*pub][pub:1]]
/// The global state within threads 
pub use self::db::Db;

/// The job `Id` from user side
pub use self::db::Id as JobId;
// pub:1 ends here

// [[file:../runners.note::*test][test:1]]
#[test]
fn test_slotmap() {
    use slotmap::Key;
    use slotmap::SlotMap;

    let mut sm = SlotMap::new();
    let foo = sm.insert("foo"); // Key generated on insert.
    let bar = sm.insert("bar");
    dbg!(foo.data().as_ffi());
    let x = format!("{:?}", foo.data());
    dbg!(x);
    dbg!(foo.data().as_ffi());
    let x = sm.remove(foo);
    dbg!(x);
    let x = sm.remove(foo);
    dbg!(x);
}
// test:1 ends here
