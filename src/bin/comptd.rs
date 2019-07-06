// core

// [[file:~/Workspace/Programming/gosh-rs/runners/runners.note::*core][core:1]]
#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;

use rocket::Data;

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[post("/upload", format = "plain", data = "<data>")]
fn upload(data: Data) -> std::io::Result<String> {
    data.stream_to_file(std::env::temp_dir().join("upload.txt"))
        .map(|n| n.to_string())
}

#[get("/hello/<name>/<age>")]
fn hello(name: String, age: u8) -> String {
    format!("Hello, {} year old named {}!", age, name)
}

#[get("/hello/<name>")]
fn hi(name: String) -> String {
    name
}

#[get("/pid")]
fn pid() -> String {
    let pid = std::process::id();
    format!("My pid is {}", pid)
}

fn main() {
    rocket::ignite()
        .mount("/", routes![index, hello, hi, pid, upload])
        .launch();
}
// core:1 ends here
