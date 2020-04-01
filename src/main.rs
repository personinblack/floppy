#![feature(proc_macro_hygiene, decl_macro)]

extern crate chrono;
extern crate regex;
#[macro_use] extern crate rocket;
#[macro_use] extern crate failure;
#[macro_use] extern crate lazy_static;

mod file;
use file::{File, FileError};
use rocket::response::NamedFile;

use std::env::var;

lazy_static! {
    static ref URL: String = var("URL").unwrap_or(
        "http://localhost:8000/".to_string());
}

#[get("/")]
#[catch(404)]
fn index() -> String {
format!("\
[0;95m    ██████  ████
   ███▒▒███▒▒███
  ▒███ ▒▒▒  ▒███   ██████  ████████  ████████  █████ ████
 ███████    ▒███  ███▒▒███▒▒███▒▒███▒▒███▒▒███▒▒███ ▒███
▒▒▒███▒     ▒███ ▒███ ▒███ ▒███ ▒███ ▒███ ▒███ ▒███ ▒███
  ▒███      ▒███ ▒███ ▒███ ▒███ ▒███ ▒███ ▒███ ▒███ ▒███
  █████     █████▒▒██████  ▒███████  ▒███████  ▒▒███████
 ▒▒▒▒▒     ▒▒▒▒▒  ▒▒▒▒▒▒   ▒███▒▒▒   ▒███▒▒▒    ▒▒▒▒▒███
                           ▒███      ▒███       ███ ▒███
                           █████     █████     ▒▒██████
                          ▒▒▒▒▒     ▒▒▒▒▒       ▒▒▒▒▒▒

[0;1;93mPUT:
> [0;40;90m$ curl -T ./sample.txt {url}[0;1m

[0;1;93mGET:
> [0;40;90m$ curl {url}?file=8079770645379253334[0;1m
", url = *URL)
}

#[get("/?<file>")]
fn get_file(file: Result<File, FileError>) -> Result<NamedFile, String> {
    match file {
        Ok(file) => {
            file.named_file().map_err(|e| e.to_string())
        },
        Err(e) => Err(e.to_string())
    }
}

#[put("/<_name>", data = "<file>")]
fn new_file_named(file: File, _name: Option<String>) -> String {
    match file.save() {
        Ok(info) => {
            info
        },
        Err(e) => e.to_string()
    }
}

#[put("/", data = "<file>")]
fn new_file(file: File) -> String {
    new_file_named(file, None)
}

fn main() {
    std::env::set_var("ROCKET_PORT",
                      std::env::var("PORT").unwrap_or("8000".into()));
    rocket::ignite()
        .register(catchers![index])
        .mount("/", routes![index, get_file, new_file,
               new_file_named])
        .launch();
}
