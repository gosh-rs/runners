// #![feature(async_await)]

// pub mod local;
// pub mod client;
// pub mod server;
// pub mod session;

pub(crate) mod common {
    pub use gosh_core::*;

    pub use gut::prelude::*;
    pub use structopt::StructOpt;

    /// Return current timestamp string
    pub fn timestamp_now() -> String {
        use chrono::prelude::*;
        let now: DateTime<Local> = Local::now();
        format!("{}", now)
    }
}
