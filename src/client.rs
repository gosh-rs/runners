// [[file:~/Workspace/Programming/gosh-rs/runner/runners.note::*imports][imports:1]]
use std::path::{Path, PathBuf};

use crate::common::*;
use crate::server::*;
// imports:1 ends here

// [[file:~/Workspace/Programming/gosh-rs/runner/runners.note::*base][base:1]]
#[derive(Clone, Debug)]
pub struct Client {
    server_addr: String,
}

impl Default for Client {
    fn default() -> Self {
        Self {
            server_addr: format!("http://{}", DEFAULT_SERVER_ADDRESS),
        }
    }
}

impl Client {
    /// Create a client with specific server address.
    pub fn new(addr: &str) -> Self {
        let server_addr = if addr.starts_with("http://") {
            addr.into()
        } else {
            format!("http://{}", addr)
        };

        Self { server_addr }
    }
}
// base:1 ends here

// [[file:~/Workspace/Programming/gosh-rs/runner/runners.note::*core][core:1]]
impl Client {
    pub fn server_address(&self) -> &str {
        self.server_addr.as_ref()
    }

    /// Request server to delete a job from queue.
    pub fn delete_job(&self, id: JobId) -> Result<()> {
        let url = format!("{}/jobs/{}", self.server_addr, id);
        let new = reqwest::blocking::Client::new().delete(&url).send()?;
        dbg!(new);

        Ok(())
    }

    /// Wait job to be done.
    pub fn wait_job(&self, id: JobId) -> Result<()> {
        let url = format!("{}/jobs/{}", self.server_addr, id);

        // NOTE: the default request timeout is 30 seconds. Here we disable
        // timeout using reqwest builder.
        //
        let new = reqwest::blocking::Client::builder()
            // .timeout(Duration::from_millis(500))
            .timeout(None)
            .build()
            .unwrap()
            .get(&url)
            .send()?;

        dbg!(new);

        Ok(())
    }

    /// Request server to create a job.
    pub fn create_job(&self, script: &str) -> Result<()> {
        let url = format!("{}/jobs/", self.server_addr);
        let job = Job::new(script);
        let new = reqwest::blocking::Client::new().post(&url).json(&job).send()?;
        dbg!(new);

        Ok(())
    }

    /// Request server to list current jobs in queue.
    pub fn list_jobs(&self) -> Result<()> {
        let url = format!("{}/jobs", self.server_addr);
        let x = reqwest::blocking::get(&url)?.text()?;
        dbg!(x);
        Ok(())
    }

    /// Request server to list files of specified job `id`.
    pub fn list_job_files(&self, id: JobId) -> Result<()> {
        let url = format!("{}/jobs/{}/files", self.server_addr, id);
        let x = reqwest::blocking::get(&url)?.text()?;
        dbg!(x);
        Ok(())
    }

    /// Download a job file from the server.
    pub fn get_job_file(&self, id: JobId, fname: &str) -> Result<()> {
        let url = format!("{}/jobs/{}/files/{}", self.server_addr, id, fname);
        let mut resp = reqwest::blocking::get(&url)?;
        let mut f = std::fs::File::create(fname)?;
        let m = resp.copy_to(&mut f)?;
        info!("copyed {} bytes.", m);

        Ok(())
    }

    /// Upload a job file to the server.
    pub fn put_job_file<P: AsRef<Path>>(&self, id: JobId, path: P) -> Result<()> {
        use std::io::*;

        let path = path.as_ref();
        assert!(path.is_file(), "{}: is not a file!", path.display());

        if let Some(fname) = &path.file_name() {
            let fname = fname.to_str().expect("invalid filename");
            let url = format!("{}/jobs/{}/files/{}", self.server_addr, id, fname);

            // read the whole file into bytes
            let mut bytes = vec![];
            let mut f = std::fs::File::open(path)?;
            f.read_to_end(&mut bytes)?;

            // send the raw bytes using PUT request
            let res = reqwest::blocking::Client::new().put(&url).body(bytes).send()?;
        } else {
            bail!("{}: not a file!", path.display());
        }

        Ok(())
    }

    /// Shutdown app server. This will kill all running processes and remove all
    /// job files.
    pub fn shutdown_server(&self) -> Result<()> {
        let url = format!("{}/jobs", self.server_addr);
        let new = reqwest::blocking::Client::new().delete(&url).send()?;
        dbg!(new);

        Ok(())
    }
}
// core:1 ends here
