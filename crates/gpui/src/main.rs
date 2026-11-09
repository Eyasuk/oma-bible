//! GPUI frontend for Oma Bible.

mod actions;
mod app;
mod reader;
mod theme;

use app::run;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
