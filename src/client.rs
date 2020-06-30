// [[file:../runners.note::*imports][imports:1]]
use std::path::{Path, PathBuf};

use crate::common::*;
use crate::server::*;
// imports:1 ends here

// [[file:../runners.note::*base][base:1]]
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

// [[file:../runners.note::*core][core:1]]
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

// [[file:../runners.note::*pub/cli][pub/cli:1]]
use gosh_core::gut::{cli::*, prelude::*};

/// A commander for interactive interpreter
#[derive(Default)]
pub struct Command {
    client: Option<Client>,
}

impl Command {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }
}

#[derive(StructOpt, Debug)]
#[structopt(setting = structopt::clap::AppSettings::VersionlessSubcommands)]
pub enum Action {
    /// Quit REPL shell.
    #[structopt(name = "quit", alias = "q", alias = "exit")]
    Quit {},

    /// Show available commands.
    #[structopt(name = "help", alias = "h", alias = "?")]
    Help {},

    /// List job/jobs submited in the server.
    #[structopt(name = "ls", alias = "l", alias = "ll")]
    List {
        /// Job id
        #[structopt(name = "JOB-ID")]
        id: Option<JobId>,
    },

    /// Request to delete a job from the server.
    #[structopt(name = "delete", alias = "del")]
    Delete {
        /// Job id
        #[structopt(name = "JOB-ID")]
        id: JobId,
    },

    /// Wait until job is done.
    #[structopt(name = "wait")]
    Wait {
        /// Job id
        #[structopt(name = "JOB-ID")]
        id: JobId,
    },

    /// Submit a job to the server.
    #[structopt(name = "submit", alias = "sub")]
    Submit {
        /// Set script file.
        #[structopt(name = "SCRIPT-FILE", parse(from_os_str))]
        script_file: PathBuf,
    },

    /// Download a job file from the server.
    #[structopt(name = "get", alias = "download")]
    Get {
        /// Job file name to be downloaded from the server.
        #[structopt(name = "FILE-NAME")]
        file_name: String,

        /// Job id
        #[structopt(name = "JOB-ID", long = "id")]
        id: JobId,
    },

    ///Shutdown the remote server.
    #[structopt(name = "shutdown")]
    Shutdown {},

    /// Upload a job file to the server.
    #[structopt(name = "put", alias = "upload")]
    Put {
        /// Job file name to be uploaded to the server.
        #[structopt(name = "FILE-NAME")]
        file_name: String,

        /// Job id
        #[structopt(name = "JOB-ID", long = "id")]
        id: JobId,
    },

    /// Connect to app server.
    #[structopt(name = "connect")]
    Connect {
        /// Application server.
        #[structopt(name = "SERVER-ADDRESS")]
        server_address: Option<String>,
    },
}

impl Command {
    pub fn apply(&mut self, action: &Action) -> Result<()> {
        match action {
            Action::Connect { server_address } => {
                let c = if let Some(addr) = &server_address {
                    Client::new(addr)
                } else {
                    Client::default()
                };
                println!("connected to {}.", c.server_address());
                self.client = Some(c);
            }
            Action::List { id } => {
                let client = self.client()?;
                if let Some(id) = id {
                    client.list_job_files(*id)?;
                } else {
                    client.list_jobs()?;
                }
            }
            Action::Submit { script_file } => {
                use std::io::Read;

                let client = self.client()?;
                let mut f = std::fs::File::open(script_file)?;
                let mut buf = String::new();
                let _ = f.read_to_string(&mut buf)?;
                client.create_job(&buf)?;
            }
            Action::Delete { id } => {
                let client = self.client()?;
                client.delete_job(*id)?;
            }
            Action::Wait { id } => {
                let client = self.client()?;
                client.wait_job(*id)?;
            }
            Action::Get { file_name, id } => {
                let client = self.client()?;
                client.get_job_file(*id, file_name)?;
            }
            Action::Put { file_name, id } => {
                let client = self.client()?;
                client.put_job_file(*id, file_name)?;
            }
            Action::Shutdown {} => {
                let client = self.client()?;
                client.shutdown_server()?;
            }
            _ => {
                eprintln!("not implemented yet.");
            }
        }

        Ok(())
    }

    // a quick wrapper to extract client
    fn client(&mut self) -> Result<&mut Client> {
        if let Some(client) = self.client.as_mut() {
            Ok(client)
        } else {
            bail!("App server not connected.");
        }
    }
}
// pub/cli:1 ends here
