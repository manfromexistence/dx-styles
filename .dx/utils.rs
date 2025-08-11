use std::path::{Path, PathBuf};
use colored::Colorize;
use walkdir::Walkdir;

pub fn find_code_files(dir: &Path