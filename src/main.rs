mod header;
mod platform;
pub use crate::platform::{dimensions, dimensions_stderr, dimensions_stdin, dimensions_stdout};

fn main() {
    header::render("Base");
    header::render("Modules");
    header::render("Layout");
    header::render("State");
    header::render("Theme");
}