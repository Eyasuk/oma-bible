//! GPUI frontend for Oma Bible.

mod actions;
mod app;
mod chrome;
mod explore;
mod marker;
mod reader;
mod rtl;
mod theme;
mod tools;

use app::run;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
