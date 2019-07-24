// imports

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*imports][imports:1]]
use std::path::{Path, PathBuf};

use linefeed::{Interface, ReadResult};

use runners::client::*;
use runners::common::*;
// imports:1 ends here

// commands

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*commands][commands:1]]
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
#[structopt(raw(setting = "structopt::clap::AppSettings::VersionlessSubcommands"))]
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
        id: Option<u64>,
    },

    /// Request to delete a job from the server.
    #[structopt(name = "delete", alias = "del")]
    Delete {
        /// Job id
        #[structopt(name = "JOB-ID")]
        id: u64,
    },

    /// Wait until job is done.
    #[structopt(name = "wait")]
    Wait {
        /// Job id
        #[structopt(name = "JOB-ID")]
        id: u64,
    },

    /// Submit a job to the server.
    #[structopt(name = "submit", alias = "sub")]
    Submit {
        /// Job id
        #[structopt(name = "JOB-ID")]
        id: u64,

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
        id: u64,
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
        id: u64,
    },

    /// Connect to app server.
    #[structopt(name = "connect")]
    Connect {
        /// Application server.
        #[structopt(name = "SERVER-ADDRESS")]
        server_address: Option<String>,
    },
}
// commands:1 ends here

// core

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*core][core:1]]
impl Command {
    fn apply(&mut self, action: &Action) -> Result<()> {
        match action {
            Action::Connect { server_address } => {
                if let Some(addr) = &server_address {
                    unimplemented!()
                } else {
                    let c = Client::default();
                    println!("connected to {}.", c.server_address());
                    self.client = Some(c);
                }
            }
            Action::List { id } => {
                let client = self.client()?;
                if let Some(id) = id {
                    client.list_job_files(*id)?;
                } else {
                    client.list_jobs()?;
                }
            }
            Action::Submit { id, script_file } => {
                use std::io::Read;

                let client = self.client()?;
                let mut f = std::fs::File::open(script_file)?;
                let mut buf = String::new();
                let _ = f.read_to_string(&mut buf)?;
                client.create_job(*id, &buf)?;
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
// core:1 ends here

// main

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*main][main:1]]
fn main() -> CliResult {
    let interface = Interface::new("application runner client")?;

    let version = env!("CARGO_PKG_VERSION");
    println!("This is the rusty gosh shell version {}.", version);
    println!("Enter \"help\" or \"?\" for a list of commands.");
    println!("Press Ctrl-D or enter \"quit\" or \"q\" to exit.");
    println!("");

    interface.set_prompt("app> ")?;

    let mut command = Command::new();
    while let ReadResult::Input(line) = interface.read_line()? {
        let line = line.trim();
        if !line.is_empty() {
            interface.add_history(line.to_owned());

            let mut args: Vec<_> = line.split_whitespace().collect();
            args.insert(0, "app>");

            match Action::from_iter_safe(&args) {
                // show subcommands
                Ok(Action::Help {}) => {
                    let mut app = Action::clap();
                    app.print_help();
                    println!("");
                }

                Ok(Action::Quit {}) => {
                    break;
                }

                // apply subcommand
                Ok(x) => {
                    if let Err(e) = command.apply(&x) {
                        eprintln!("{:?}", e);
                    }
                }

                // show subcommand usage
                Err(e) => {
                    println!("{}", e.message);
                }
            }
        } else {
            println!("");
        }
    }

    Ok(())
}
// main:1 ends here
