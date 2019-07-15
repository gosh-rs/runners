// imports

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*imports][imports:1]]
use std::sync::Arc;
use std::sync::Mutex;

use quicli::prelude::*;
use serde::{Serialize, Deserialize};

use warp::Filter;

use crate::server::*;
// imports:1 ends here

// base

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*base][base:1]]
/// So we don't have to tackle how different database work, we'll just use
/// a simple in-memory DB, a vector synchronized by a mutex.
type Db = Arc<Mutex<Vec<Job>>>;

#[derive(Clone, Debug, Copy, Deserialize, Serialize)]
struct Job {
    id: u64
}
// base:1 ends here

// api

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*api][api:1]]
/// GET /jobs
fn list_jobs(db: Db) -> impl warp::Reply {
    // Just return a JSON array of all Todos.
    info!("list jobs");
    warp::reply::json(&*db.lock().unwrap())
}

/// POST /jobs with JSON body
fn create_job(create: Job, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    info!("create_job: {:?}", create);

    let mut vec = db.lock().unwrap();

    for job in vec.iter() {
        if job.id == create.id {
            debug!("    -> id already exists: {}", create.id);
            // Job with id already exists, return `400 BadRequest`.
            return Ok(warp::http::StatusCode::BAD_REQUEST);
        }
    }

    // No existing Job with id, so insert and return `201 Created`.
    vec.push(create);

    Ok(warp::http::StatusCode::CREATED)
}

/// DELETE /jobs/:id
fn delete_job(id: u64, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    info!("delete_job: id={}", id);

    let mut vec = db.lock().unwrap();

    let len = vec.len();
    vec.retain(|job| {
        // Retain all Jobs that aren't this id...
        // In other words, remove all that *are* this id...
        job.id != id
    });

    // If the vec is smaller, we found and deleted a Job!
    let deleted = vec.len() != len;

    if deleted {
        // respond with a `204 No Content`, which means successful,
        // yet no body expected...
        Ok(warp::http::StatusCode::NO_CONTENT)
    } else {
        debug!("    -> job id not found!");
        // Reject this request with a `404 Not Found`...
        Err(warp::reject::not_found())
    }
}

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
// api:1 ends here

// core

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*core][core:1]]
pub fn test() {
    // These are some `Filter`s that several of the endpoints share,
    // so we'll define them here and reuse them below...

    // Turn our "state", our db, into a Filter so we can combine it
    // easily with others...
    let db = Arc::new(Mutex::new(Vec::<Job>::new()));
    let db = warp::any().map(move || db.clone());

    // Just the path segment "jobs"...
    let jobs = warp::path("jobs");

    // Combined with `end`, this means nothing comes after "jobs".
    // So, for example: `GET /jobs`, but not `GET /jobs/32`.
    let jobs_index = jobs.and(warp::path::end());

    // Combined with an id path parameter, for refering to a specific Job.
    // For example, `POST /jobs/32`, but not `POST /jobs/32/something-more`.
    let jobs_id = jobs.and(warp::path::param::<u64>()).and(warp::path::end());

    // When accepting a body, we want a JSON body
    // (and to reject huge payloads)...
    let json_body = warp::body::content_length_limit(1024 * 16).and(warp::body::json());

    // Next, we'll define each our 4 endpoints:

    // `GET /jobs`
    let list = warp::get2().and(jobs_index).and(db.clone()).map(list_jobs);

    // `POST /jobs`
    let create = warp::post2()
        .and(jobs_index)
        .and(json_body)
        .and(db.clone())
        .and_then(create_job);

    // `PUT /jobs/:id`
    let update = warp::put2()
        .and(jobs_id)
        .and(json_body)
        .and(db.clone())
        .and_then(update_job);

    // `DELETE /jobs/:id`
    let delete = warp::delete2()
        .and(jobs_id)
        .and(db.clone())
        .and_then(delete_job);

    // Combine our endpoints, since we want requests to match any of them:
    let api = list.or(create).or(update).or(delete);

    // View access logs by setting `RUST_LOG=jobs`.
    let routes = api.with(warp::log("jobs"));

    info!("here");

    // Start up the server...
    warp::serve(routes).run(([127, 0, 0, 1], 3030));
}
// core:1 ends here
