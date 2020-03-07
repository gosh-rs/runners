// [[file:../runners.note::*lib.rs][lib.rs:1]]
mod process;
mod session;

//pub mod adhoc;
pub mod client;
pub mod server;
pub mod local;

pub(crate) mod common {
    pub use gosh_core::*;

    pub use gut::prelude::*;

    /// Return current timestamp string
    pub fn timestamp_now() -> String {
        use chrono::prelude::*;
        let now: DateTime<Local> = Local::now();
        format!("{}", now)
    }
}
// lib.rs:1 ends here
