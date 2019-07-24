// imports

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*imports][imports:1]]
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;

use quicli::prelude::*;
use serde::{Deserialize, Serialize};

use tokio::prelude::*;
use warp::*;

use crate::server::*;
// imports:1 ends here

// base

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*base][base:1]]
/// So we don't have to tackle how different database work, we'll just use
/// a simple in-memory DB, a vector synchronized by a mutex.
type Db = Arc<Mutex<Vec<Job>>>;

#[derive(Clone, Debug)]
pub enum JobStatus {
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

#[derive(Debug, Deserialize, Serialize)]
pub struct Job {
    id: u64,

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
    session: Option<tokio_process::Child>,
}

impl Job {
    ///
    /// Construct a Job with shell script of job run_file.
    ///
    /// # Parameters
    ///
    /// * script: the content of the run script of the job.
    ///
    pub fn new(id: u64, script: &str) -> Self {
        Self {
            id,

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

    /// Set content of job stdin stream.
    pub fn with_stdin(mut self, content: &str) -> Self {
        self.input = content.into();
        self
    }

    /// Return full path to computation output file (stdout).
    pub fn out_file(&self) -> PathBuf {
        if let Some(d) = &self.wrk_dir {
            d.path().join(&self.out_file)
        } else {
            panic!("no working dir!");
        }
    }

    /// Return full path to computation output file (stdout).
    pub fn err_file(&self) -> PathBuf {
        if let Some(d) = &self.wrk_dir {
            d.path().join(&self.err_file)
        } else {
            panic!("no working dir!");
        }
    }

    /// Return full path to computation output file (stdout).
    pub fn inp_file(&self) -> PathBuf {
        if let Some(d) = &self.wrk_dir {
            d.path().join(&self.inp_file)
        } else {
            panic!("no working dir!");
        }
    }

    /// Return full path to computation output file (stdout).
    pub fn run_file(&self) -> PathBuf {
        if let Some(d) = &self.wrk_dir {
            d.path().join(&self.run_file)
        } else {
            panic!("no working dir!");
        }
    }
}
// base:1 ends here

// core

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*core][core:1]]
impl Job {
    // run command in background
    fn start(&mut self) {
        use crate::local::Runner;

        use tokio::prelude::*;
        use tokio_process::CommandExt;

        if let Some(d) = &self.wrk_dir {
            info!("job {}, work direcotry: {}", self.id, d.path().display());

            let runner = Runner::new(&self.run_file());
            let mut child = runner
                .build_command()
                .current_dir(d)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn_async()
                .expect("spawn command session");

            let mut stdin = child
                .stdin()
                .take()
                .expect("child did not have a handle to stdout");
            let stdout = child
                .stdout()
                .take()
                .expect("child did not have a handle to stdout");
            let stderr = child
                .stderr()
                .take()
                .expect("child did not have a handle to stderr");

            // NOTE: suppose stdin stream is small.
            stdin.write_all(self.input.as_bytes()).expect("write stdin");

            // redirect stdout and stderr to files for user inspection.
            let save_stdout = tokio::fs::File::create(self.out_file())
                .and_then(move |f| tokio::io::copy(stdout, f))
                .map(move |_| ())
                .map_err(|e| panic!("error while saving stdout: {}", e));
            let save_stderr = tokio::fs::File::create(self.err_file())
                .and_then(move |f| tokio::io::copy(stderr, f))
                .map(|_| ())
                .map_err(|e| panic!("error while saving stderr: {}", e));

            tokio::spawn(save_stderr);
            tokio::spawn(save_stdout);

            let sid = child.id();
            info!("command running in session {}", sid);
            self.session = Some(child);
        } else {
            panic!("temp dir not set!");
        }
    }

    /// Terminate background command session.
    fn terminate(&mut self) {
        if let Some(child) = &mut self.session {
            let sid = child.id();
            crate::local::terminate_session(sid).expect("term session");
            info!(
                "Job {} with command session {} has been terminated.",
                self.id, sid
            );
        } else {
            debug!("Job not started yet.");
        }
    }

    fn wait(&mut self) {
        if let Some(mut child) = self.session.take() {
            child.wait_with_output().wait();
        } else {
            error!("Job not started yet.");
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        debug!("Dropping job {}!", self.id);
        self.terminate();
    }
}
// core:1 ends here

// create job

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*create%20job][create job:1]]
/// POST /jobs with JSON body
fn create_job(mut create: Job, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    info!("create_job: {:?}", create);

    let mut vec = db.lock().unwrap();

    for job in vec.iter() {
        if job.id == create.id {
            debug!("    -> id already exists: {}", create.id);
            // Job with id already exists, return `400 BadRequest`.
            return Ok(warp::http::StatusCode::BAD_REQUEST);
        }
    }

    // run command
    // use crate::local::Runner;
    // let mut job = Job::new(create.id);
    create.build();
    create.start();

    // No existing Job with id, so insert and return `201 Created`.
    vec.push(create);

    Ok(warp::http::StatusCode::CREATED)
}
// create job:1 ends here

// delete job

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*delete%20job][delete job:1]]
/// DELETE /jobs/:id
fn delete_job(id: u64, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    info!("delete_job: id={}", id);

    let mut vec = db.lock().unwrap();
    if let Some(i) = vec.iter().position(|j| j.id == id) {
        let _ = vec.remove(i);

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

// update job

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*update%20job][update job:1]]
/// PUT /jobs/:id with JSON body
fn update_job(id: u64, update: Job, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    debug!("update_job: id={}, job={:?}", id, update);
    let mut vec = db.lock().unwrap();

    // Look for the specified Job...
    for job in vec.iter_mut() {
        if job.id == id {
            *job = update;
            return Ok(warp::reply());
        }
    }
    debug!("    -> job id not found!");

    // If the for loop didn't return OK, then the ID doesn't exist...
    Err(warp::reject::not_found())
}
// update job:1 ends here

// list job

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*list%20job][list job:1]]
/// GET /jobs
/// List jobs in queue
fn list_jobs(db: Db) -> impl warp::Reply {
    info!("list jobs");
    warp::reply::json(&*db.lock().unwrap())
}

/// GET /jobs/:id/files
///
/// List files in job working directory
fn list_job_files(id: u64, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    let mut vec = db.lock().unwrap();
    info!("list files for job {}", id);

    // List files for the specified Job...
    for job in vec.iter() {
        if job.id == id {
            if let Some(d) = &job.wrk_dir {
                let mut list = vec![];
                for entry in std::fs::read_dir(&d).expect("list dir") {
                    if let Ok(entry) = entry {
                        let p = entry.path();
                        if p.is_file() {
                            list.push(p);
                        }
                    }
                }
                return Ok(warp::reply::json(&list));
            }
        }
    }

    // If the for loop didn't return OK, then the ID doesn't exist...
    Err(warp::reject::not_found())
}
// list job:1 ends here

// job files

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*job%20files][job files:1]]
/// `GET` /jobs/:id/files/:file
pub fn get_job_file(id: u64, file: String, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    debug!("get_job_file: id={}", id);
    let mut vec = db.lock().unwrap();

    // Look for the specified Job...
    for job in vec.iter() {
        if job.id == id {
            if let Some(d) = &job.wrk_dir {
                let p = d.path().join(&file);
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
            } else {
                error!("no working dir!");
            }
        }
    }

    // If the for loop didn't return OK, then the ID doesn't exist...
    Err(warp::reject::not_found())
}

/// `PUT` /jobs/:id/files/:file
pub fn put_job_file(
    id: u64,
    file: String,
    db: Db,
    body: warp::body::FullBody,
) -> Result<impl warp::Reply, warp::Rejection> {
    debug!("put_job_file: id={}", id);
    let mut vec = db.lock().unwrap();
    // Look for the specified Job...
    for job in vec.iter() {
        if job.id == id {
            if let Some(d) = &job.wrk_dir {
                let p = d.path().join(&file);
                info!("client request to put a file: {}", p.display());
                match std::fs::File::create(p) {
                    Ok(mut f) => {
                        let _ = f.write_all(body.bytes());
                        return Ok(warp::reply());
                    }
                    Err(e) => {
                        error!("{}", e);
                    }
                }
            } else {
                error!("no working dir!");
            }
        }
    }

    // If the for loop didn't return OK, then the ID doesn't exist...
    Err(warp::reject::not_found())
}
// job files:1 ends here

// shutdown

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*shutdown][shutdown:1]]
/// DELETE /jobs
/// shutdown server
fn shutdown_server(db: Db) -> impl warp::Reply {
    info!("shudown server now ...");
    // drop jobs
    let mut jobs = db.lock().unwrap();
    jobs.clear();

    send_signal(tokio_signal::unix::SIGINT);
    warp::http::StatusCode::NO_CONTENT
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

// wait job

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*wait%20job][wait job:1]]
/// GET /jobs/:id
fn wait_job(id: u64, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    info!("wait_job: id={}", id);

    let mut jobs = db.lock().unwrap();
    if let Some(i) = jobs.iter().position(|j| j.id == id) {
        jobs[i].wait();
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

// core

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*core][core:1]]
pub fn run() {
    // These are some `Filter`s that several of the endpoints share,
    // so we'll define them here and reuse them below...

    // Turn our "state", our db, into a Filter so we can combine it
    // easily with others...
    let db_raw = Arc::new(Mutex::new(Vec::<Job>::new()));
    let db = warp::any().map(move || db_raw.clone());

    // Just the path segment "jobs"...
    let jobs = warp::path("jobs");

    // Combined with `end`, this means nothing comes after "jobs".
    // So, for example: `GET /jobs`, but not `GET /jobs/32`.
    let jobs_index = jobs.and(warp::path::end());

    // Combined with an id path parameter, for refering to a specific Job.
    // For example, `POST /jobs/32`, but not `POST /jobs/32/something-more`.
    let job_id = jobs.and(warp::path::param::<u64>()).and(warp::path::end());

    // jobs/:id/files
    let job_dir = path!("jobs" / u64 / "files").and(warp::path::end());

    // jobs/:id/files/job.out
    let job_file = path!("jobs" / u64 / "files" / String).and(warp::path::end());

    // When accepting a body, we want a JSON body
    // (and to reject huge payloads)...
    let json_body = warp::body::content_length_limit(1024 * 16).and(warp::body::json());

    // Next, we'll define each our endpoints:

    // `GET /jobs`
    let list = warp::get2().and(jobs_index).and(db.clone()).map(list_jobs);

    // `DELETE /jobs`
    let shutdown = warp::delete2()
        .and(jobs_index)
        .and(db.clone())
        .map(shutdown_server);

    // `POST /jobs`
    let create = warp::post2()
        .and(jobs_index)
        .and(json_body)
        .and(db.clone())
        .and_then(create_job);

    // `PUT /jobs/:id`
    let update = warp::put2()
        .and(job_id)
        .and(json_body)
        .and(db.clone())
        .and_then(update_job);

    // `DELETE /jobs/:id`
    let delete = warp::delete2()
        .and(job_id)
        .and(db.clone())
        .and_then(delete_job);

    // `GET` /jobs/:id/files
    let list_dir = warp::get2()
        .and(job_dir)
        .and(db.clone())
        .and_then(list_job_files);

    // `GET /jobs/:id`
    let wait = warp::get2()
        .and(job_id)
        .and(db.clone())
        .and_then(wait_job);

    // `GET` /jobs/:id/files/:file
    let get_file = warp::get2()
        .and(job_file)
        .and(db.clone())
        .and_then(get_job_file);

    // `PUT` /jobs/:id/files/:file
    let put_file = warp::put2()
        .and(job_file)
        .and(db.clone())
        .and(warp::body::concat())
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

    // setup signal handler
    let sig = tokio_signal::ctrl_c()
        .flatten_stream()
        .into_future()
        .map(move |_| {
            println!("User interrupted.");
            let _ = tx.send(());
        });

    let addr = "127.0.0.1:3030"
        .parse::<std::net::SocketAddr>()
        .expect("server address");
    let (addr, server) = server.bind_with_graceful_shutdown(addr, rx);

    dbg!(addr);

    // Spawn the server into a runtime
    let fut = sig.select2(server).map(|_| ()).map_err(|_| ());
    tokio::run(fut);
}
// core:1 ends here
