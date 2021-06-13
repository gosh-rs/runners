// [[file:../runners.note::*mods][mods:1]]
mod client;
mod job;
mod local;
mod process;
mod server;
mod session;
// mods:1 ends here

// [[file:../runners.note::*pub][pub:1]]
// shared imports between mods
pub(crate) mod common {
    pub use gosh_core::*;
    pub use gut::prelude::*;
    pub use std::path::{Path, PathBuf};

    /// Return current timestamp string
    pub fn timestamp_now() -> String {
        use chrono::prelude::*;
        let now: DateTime<Local> = Local::now();
        format!("{}", now)
    }
}

// for command line binaries
pub use client::enter_main as client_enter_main;
pub use client::Client;
pub use local::enter_main as local_enter_main;
pub use server::enter_main as server_enter_main;

/// Some extension traits
pub mod prelude {
    pub use crate::process::ProcessGroupExt;
}
// pub:1 ends here

// [[file:../runners.note::*docs][docs:1]]
#[cfg(feature = "adhoc")] 
/// Documentation for local development
pub mod docs {
    pub use crate::job::*;
}
// docs:1 ends here
