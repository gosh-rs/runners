// [[file:../runners.note::*imports][imports:1]]
// #![deny(warnings)]
use crate::common::*;
use crate::job::{Db, Job, JobId};

pub const DEFAULT_SERVER_ADDRESS: &str = "127.0.0.1:3030";
// imports:1 ends here

// [[file:../runners.note::*server][server:1]]
use std::net::{SocketAddr, ToSocketAddrs};

/// Computation server.
pub struct Server {
    address: SocketAddr,
}

impl Server {
    fn new(addr: &str) -> Self {
        let addrs: Vec<_> = addr.to_socket_addrs().expect("bad address").collect();

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
// server:1 ends here

// [[file:../runners.note::*imports][imports:1]]
use bytes::Bytes;
use warp::*;
// imports:1 ends here

// [[file:../runners.note::*create job][create job:1]]
/// POST /jobs with JSON body
async fn create_job(create: Job, mut db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    let jid = db.insert_job(create).await;
    Ok(warp::reply::json(&jid))
}
// create job:1 ends here

// [[file:../runners.note::*delete job][delete job:1]]
/// DELETE /jobs/:id
async fn delete_job(id: JobId, mut db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    match db.delete_job(id).await {
        Ok(_) => {
            // respond with a `204 No Content`, which means successful,
            // yet no body expected...
            Ok(warp::http::StatusCode::NO_CONTENT)
        }
        Err(e) => {
            error!("cannot delete job {}: {}", id, e);
            // Reject this request with a `404 Not Found`...
            Err(warp::reject::not_found())
        }
    }
}
// delete job:1 ends here

// [[file:../runners.note::*update job][update job:1]]
/// PUT /jobs/:id with JSON body
async fn update_job(id: JobId, update: Job, mut db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    match db.update_job(id, update).await {
        Ok(_) => Ok(warp::reply()),
        Err(e) => {
            error!("{}", e);
            Err(warp::reject::not_found())
        }
    }
}
// update job:1 ends here

// [[file:../runners.note::*list job][list job:1]]
/// List jobs in queue
///
/// GET /jobs
async fn list_jobs(db: Db) -> Result<impl warp::Reply, std::convert::Infallible> {
    info!("list jobs");
    let list = db.get_job_list().await;
    Ok(warp::reply::json(&list))
}

/// List files in job working directory
///
/// GET /jobs/:id/files
async fn list_job_files(id: JobId, mut db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    match db.list_job_files(id).await {
        Ok(list) => Ok(warp::reply::json(&list)),
        Err(e) => {
            error!("{}", e);
            Err(warp::reject::not_found())
        }
    }
}
// list job:1 ends here

// [[file:../runners.note::*job files][job files:1]]
// `GET` /jobs/:id/files/:file
async fn get_job_file(id: JobId, file: String, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    match db.get_job_file(id, file.as_ref()).await {
        Ok(buffer) => Ok(buffer),
        Err(e) => {
            Err(warp::reject::not_found())
        }
    }
}

/// `PUT` /jobs/:id/files/:file
async fn put_job_file(id: JobId, file: String, mut db: Db, body: Bytes) -> Result<impl warp::Reply, warp::Rejection> {
    match db.put_job_file(id, file, body).await {
        Ok(_) => Ok(warp::reply()),
        Err(e) => {
            error!("{}", e);
            // If the for loop didn't return OK, then the ID doesn't exist...
            Err(warp::reject::not_found())
        }
    }
}
// job files:1 ends here

// [[file:../runners.note::*shutdown][shutdown:1]]
// shutdown server
// DELETE /jobs
async fn shutdown_server(mut db: Db) -> Result<impl warp::Reply, std::convert::Infallible> {
    info!("shudown server now ...");
    db.clear_jobs();

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
    match db.wait_job(id).await {
        Ok(_) => {
            // respond with a `204 No Content`, which means successful,
            // yet no body expected...
            Ok(warp::http::StatusCode::NO_CONTENT)
        }
        Err(e) => {
            // Reject this request with a `404 Not Found`...
            Err(warp::reject::not_found())
        }
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
        let db = Db::new();
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
