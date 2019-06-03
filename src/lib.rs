// lib.rs
// :PROPERTIES:
// :header-args: :tangle src/lib.rs
// :END:

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*lib.rs][lib.rs:1]]
pub mod local;

pub mod common {
    pub use quicli::prelude::*;
    pub type Result<T> = ::std::result::Result<T, Error>;

    /// Return current timestamp string
    pub fn timestamp_now() -> String {
        use chrono::prelude::*;
        let now: DateTime<Local> = Local::now();
        format!("{}", now)
    }
}
// lib.rs:1 ends here
