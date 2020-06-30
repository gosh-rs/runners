// [[file:../runners.note::*imports][imports:1]]
// #![deny(warnings)]

use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::prelude::*;

use crate::common::*;
// imports:1 ends here

// [[file:../runners.note::*base/job][base/job:1]]
pub const DEFAULT_SERVER_ADDRESS: &str = "127.0.0.1:3030";

#[derive(Clone, Debug)]
enum JobStatus {
    NotStarted,
    Running,
    /// failure code
    Failure(i32),
    Success,
}

impl Default for JobStatus {
    fn default() -> Self {
        Self::NotStarted
    }
}

pub type JobId = usize;

#[derive(Debug, Deserialize, Serialize)]
pub struct Job {
    out_file: String,

    err_file: String,

    run_file: String,

    script: String,

    input: String,

    inp_file: String,

    #[serde(skip)]
    status: JobStatus,

    #[serde(skip)]
    wrk_dir: Option<tempfile::TempDir>,

    // command session
    #[serde(skip)]
    session: Option<tokio::process::Child>,
}
// base/job:1 ends here

// [[file:../runners.note::*base/db][base/db:1]]
use std::sync::Arc;
use tokio::sync::Mutex;

// type Jobs = Vec<Job>;
type Jobs = slab::Slab<Job>;

/// So we don't have to tackle how different database work, we'll just use
/// a simple in-memory DB, a vector synchronized by a mutex.
type Db = Arc<Mutex<Jobs>>;

fn blank_db() -> Db {
    Arc::new(Mutex::new(Jobs::new()))
}
// base/db:1 ends here

// [[file:../runners.note::*base/server][base/server:1]]
use std::net::{SocketAddr, ToSocketAddrs};

/// Computation server.
pub struct Server {
    address: SocketAddr,
}

impl Server {
    fn new(addr: &str) -> Self {
        let addrs: Vec<_> = addr.to_socket_addrs().expect("bad address").collect();

        dbg!(&addrs);
        match addrs.len() {
            0 => {
                panic!("no valid server address!");
            }
            1 => Self { address: addrs[0] },
            _ => {
                let ipv4addrs: Vec<_> = addrs.iter().filter(|a| a.is_ipv4()).collect();
                if ipv4addrs.len() == 0 {
                    panic!("no valid ipv4 address: {:?}", addrs);
                } else {
                    warn!("found multiple IPV4 addresses: {:?}", ipv4addrs);
                    Self { address: *ipv4addrs[0] }
                }
            }
        }
    }
}
// base/server:1 ends here

// [[file:../runners.note::*job][job:1]]
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
            status: JobStatus::default(),
            session: None,
            wrk_dir: None,
        }
    }

    /// Set content of job stdin stream.
    fn with_stdin(mut self, content: &str) -> Self {
        self.input = content.into();
        self
    }

    /// Return full path to computation output file (stdout).
    fn out_file(&self) -> PathBuf {
        let wdir = self.wrk_dir();
        wdir.join(&self.out_file)
    }

    /// Return full path to computation output file (stdout).
    fn err_file(&self) -> PathBuf {
        let wdir = self.wrk_dir();
        wdir.join(&self.err_file)
    }

    /// Return full path to computation output file (stdout).
    fn inp_file(&self) -> PathBuf {
        let wdir = self.wrk_dir();
        wdir.join(&self.inp_file)
    }

    /// Return full path to computation output file (stdout).
    fn run_file(&self) -> PathBuf {
        let wdir = self.wrk_dir();
        wdir.join(&self.run_file)
    }

    fn wrk_dir(&self) -> &Path {
        if let Some(d) = &self.wrk_dir {
            d.path()
        } else {
            panic!("no working dir!")
        }
    }
}
// job:1 ends here

// [[file:../runners.note::*core][core:1]]
impl Job {
    /// Create runnable script file and stdin file from self.script and
    /// self.input.
    fn build(&mut self) {
        use std::fs::File;
        use std::os::unix::fs::OpenOptionsExt;

        // create working directory in scratch space.
        let wdir = tempfile::tempdir().expect("temp dir");
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
    async fn wait(&mut self) {
        if let Some(mut child) = self.session.take() {
            child.wait_with_output().await;
        } else {
            error!("Job not started yet.");
        }
    }

    /// Terminate background command session.
    fn terminate(&mut self) {
        if let Some(child) = &mut self.session {
            let sid = child.id();
            crate::process::signal_processes_by_session_id(sid, "SIGTERM").expect("term session");
            info!("Job with command session {} has been terminated.", sid);
        } else {
            debug!("Job not started yet.");
        }
    }

    /// Run command in background.
    async fn start(&mut self) -> Result<()> {
        use tokio::prelude::*;

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
        info!("command running in session {}", sid);
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

// [[file:../runners.note::*imports][imports:1]]
use warp::*;
// imports:1 ends here

// [[file:../runners.note::*create job][create job:1]]
/// POST /jobs with JSON body
async fn create_job(mut create: Job, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    info!("create_job: {:?}", create);
    let mut jobs = db.lock().await;

    // run command
    create.build();

    // Insert job into the queue.
    let jid = jobs.insert(create);
    info!("Job {} created.", jid);

    Ok(warp::reply::json(&jid))
}
// create job:1 ends here

// [[file:../runners.note::*delete job][delete job:1]]
/// DELETE /jobs/:id
async fn delete_job(id: JobId, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    info!("delete_job: id={}", id);
    let mut jobs = db.lock().await;

    if jobs.contains(id) {
        let _ = jobs.remove(id);

        // respond with a `204 No Content`, which means successful,
        // yet no body expected...
        Ok(warp::http::StatusCode::NO_CONTENT)
    } else {
        debug!("    -> job id not found!");
        // Reject this request with a `404 Not Found`...
        Err(warp::reject::not_found())
    }
}
// delete job:1 ends here

// [[file:../runners.note::*update job][update job:1]]
/// PUT /jobs/:id with JSON body
async fn update_job(id: JobId, update: Job, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    debug!("update_job: id={}, job={:?}", id, update);
    let mut jobs = db.lock().await;

    // Look for the specified Job...
    if jobs.contains(id) {
        jobs[id] = update;
        return Ok(warp::reply());
    }

    // If the for loop didn't return OK, then the ID doesn't exist...
    debug!("    -> job id not found!");
    Err(warp::reject::not_found())
}
// update job:1 ends here

// [[file:../runners.note::*list job][list job:1]]
/// List jobs in queue
///
/// GET /jobs
async fn list_jobs(db: Db) -> Result<impl warp::Reply, std::convert::Infallible> {
    info!("list jobs");
    let jobs = db.lock().await;
    let list: Vec<JobId> = jobs.iter().map(|(k, _)| k).collect();
    Ok(warp::reply::json(&list))
}

/// List files in job working directory
///
/// GET /jobs/:id/files
async fn list_job_files(id: JobId, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    let mut jobs = db.lock().await;
    info!("list files for job {}", id);

    // List files for the specified Job...
    if jobs.contains(id) {
        let mut list = vec![];
        let job = &jobs[id];
        for entry in std::fs::read_dir(job.wrk_dir()).expect("list dir") {
            if let Ok(entry) = entry {
                let p = entry.path();
                if p.is_file() {
                    list.push(p);
                }
            }
        }
        return Ok(warp::reply::json(&list));
    } else {
        // If the for loop didn't return OK, then the ID doesn't exist...
        Err(warp::reject::not_found())
    }
}
// list job:1 ends here

// [[file:../runners.note::*job files][job files:1]]
// `GET` /jobs/:id/files/:file
async fn get_job_file(id: JobId, file: String, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    debug!("get_job_file: id={}", id);
    let mut jobs = db.lock().await;

    // Look for the specified Job...
    if jobs.contains(id) {
        let job = &jobs[id];
        let p = job.wrk_dir().join(&file);
        info!("client request file: {}", p.display());

        match std::fs::File::open(p) {
            Ok(mut f) => {
                let mut buffer = Vec::new();
                f.read_to_end(&mut buffer).unwrap();
                return Ok(buffer);
            }
            Err(e) => {
                error!("{}", e);
            }
        }
    }

    // If the for loop didn't return OK, then the ID doesn't exist...
    Err(warp::reject::not_found())
}

/// `PUT` /jobs/:id/files/:file
async fn put_job_file(
    id: JobId,
    file: String,
    db: Db,
    body: bytes::Bytes,
) -> Result<impl warp::Reply, warp::Rejection> {
    debug!("put_job_file: id={}", id);
    let mut jobs = db.lock().await;
    // Look for the specified Job...
    if jobs.contains(id) {
        let job = &jobs[id];
        let p = job.wrk_dir().join(&file);
        info!("client request to put a file: {}", p.display());
        match std::fs::File::create(p) {
            Ok(mut f) => {
                let _ = f.write_all(&body);
                return Ok(warp::reply());
            }
            Err(e) => {
                error!("{}", e);
            }
        }
    }

    // If the for loop didn't return OK, then the ID doesn't exist...
    Err(warp::reject::not_found())
}
// job files:1 ends here

// [[file:../runners.note::*shutdown][shutdown:1]]
// shutdown server
// DELETE /jobs
pub async fn shutdown_server(db: Db) -> Result<impl warp::Reply, std::convert::Infallible> {
    info!("shudown server now ...");
    // drop jobs
    let mut jobs = db.lock().await;
    jobs.clear();

    send_signal(libc::SIGINT);
    Ok(warp::http::StatusCode::NO_CONTENT)
}

#[cfg(unix)]
pub fn send_signal(signal: libc::c_int) {
    use libc::{getpid, kill};
    info!("inform main thread to exit by sending signal {}.", signal);

    unsafe {
        assert_eq!(kill(getpid(), signal), 0);
    }
}
// shutdown:1 ends here

// [[file:../runners.note::*wait job][wait job:1]]
/// GET /jobs/:id
async fn wait_job(id: JobId, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    info!("wait_job: id={}", id);

    let mut jobs = db.lock().await;
    if jobs.contains(id) {
        &jobs[id].start().await;
        &jobs[id].wait().await;
        // respond with a `204 No Content`, which means successful,
        // yet no body expected...
        Ok(warp::http::StatusCode::NO_CONTENT)
    } else {
        debug!("    -> job id not found!");
        // Reject this request with a `404 Not Found`...
        Err(warp::reject::not_found())
    }
}
// wait job:1 ends here

// [[file:../runners.note::*routes][routes:1]]
impl Server {
    async fn serve(&self) {
        // These are some `Filter`s that several of the endpoints share,
        // so we'll define them here and reuse them below...

        // Turn our "state", our db, into a Filter so we can combine it
        // easily with others...
        let db = blank_db();
        let db = warp::any().map(move || db.clone());

        // Just the path segment "jobs"...
        let jobs = warp::path("jobs");

        // Combined with `end`, this means nothing comes after "jobs".
        // So, for example: `GET /jobs`, but not `GET /jobs/32`.
        let jobs_index = jobs.and(warp::path::end());

        // Combined with an id path parameter, for refering to a specific Job.
        // For example, `POST /jobs/32`, but not `POST /jobs/32/something-more`.
        let job_id = jobs.and(warp::path::param::<JobId>()).and(warp::path::end());

        // jobs/:id/files
        let job_dir = path!("jobs" / JobId / "files").and(warp::path::end());

        // jobs/:id/files/job.out
        let job_file = path!("jobs" / JobId / "files" / String).and(warp::path::end());

        // When accepting a body, we want a JSON body
        // (and to reject huge payloads)...
        let json_body = warp::body::content_length_limit(1024 * 16).and(warp::body::json());

        // Next, we'll define each our endpoints:

        // `GET /jobs`
        let list = warp::get().and(jobs_index).and(db.clone()).and_then(list_jobs);

        // `DELETE /jobs`
        let shutdown = warp::delete().and(jobs_index).and(db.clone()).and_then(shutdown_server);

        // `POST /jobs`
        let create = warp::post()
            .and(jobs_index)
            .and(json_body)
            .and(db.clone())
            .and_then(create_job);

        // `PUT /jobs/:id`
        let update = warp::put()
            .and(job_id)
            .and(json_body)
            .and(db.clone())
            .and_then(update_job);

        // `DELETE /jobs/:id`
        let delete = warp::delete().and(job_id).and(db.clone()).and_then(delete_job);

        // `GET` /jobs/:id/files
        let list_dir = warp::get().and(job_dir).and(db.clone()).and_then(list_job_files);

        // `GET /jobs/:id`
        let wait = warp::get().and(job_id).and(db.clone()).and_then(wait_job);

        // `GET` /jobs/:id/files/:file
        let get_file = warp::get().and(job_file).and(db.clone()).and_then(get_job_file);

        // `PUT` /jobs/:id/files/:file
        let put_file = warp::put()
            .and(job_file)
            .and(db.clone())
            .and(warp::body::bytes())
            .and_then(put_job_file);

        // Combine our endpoints, since we want requests to match any of them:
        let api = list
            .or(create)
            .or(update)
            .or(delete)
            .or(wait)
            .or(shutdown)
            .or(list_dir)
            .or(get_file)
            .or(put_file);

        // View access logs by setting `RUST_LOG=jobs`.
        let routes = api.with(warp::log("jobs"));
        let server = warp::serve(routes);

        // Start up the server in a scratch directory ...
        let (tx, rx) = tokio::sync::oneshot::channel();

        let (addr, server) = server.bind_with_graceful_shutdown(self.address, async {
            rx.await.ok();
        });
        dbg!(addr);

        let ctrl_c = tokio::signal::ctrl_c();
        tokio::select! {
            _ = server => {
                eprintln!("server closed");
            }
            _ = ctrl_c => {
                let _ = tx.send(());
                eprintln!("user interruption");
            }
        }
    }
}
// routes:1 ends here

// [[file:../runners.note::*pub/fn][pub/fn:1]]
/// Run local server for tests
pub(self) async fn run() {
    let addr = DEFAULT_SERVER_ADDRESS;
    let server = Server::new(addr);
    server.serve().await;
}

pub(self) async fn bind(addr: &str) {
    let server = Server::new(addr);
    server.serve().await;
}
// pub/fn:1 ends here

// [[file:../runners.note::*pub/cli][pub/cli:1]]
use gosh_core::gut;
use structopt::*;

use gut::prelude::*;

/// Application server for remote calculations.
#[derive(StructOpt, Debug)]
struct Cli {
    #[structopt(flatten)]
    verbose: gut::cli::Verbosity,

    /// Set application server address for binding.
    ///
    /// * Example
    ///
    /// - app-server localhost:3030 (default)
    /// - app-server tower:7070
    #[structopt(name = "ADDRESS")]
    address: Option<String>,
}

#[tokio::main]
pub async fn enter_main() -> Result<()> {
    let args = Cli::from_args();
    args.verbose.setup_logger();

    if let Some(addr) = args.address {
        dbg!(&addr);
        bind(&addr).await;
    } else {
        run().await;
    }

    Ok(())
}
// pub/cli:1 ends here
