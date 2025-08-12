use colored::Colorize;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn find_code_files(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| is_code_file(e.path()))
        .map(|e| e.path().canonicalize().unwrap_or_else(|_| e.path().to_path_buf()))
        .collect()
}

pub fn is_code_file(path: &Path) -> bool {
    path.extension()
        .map_or(false, |ext| ext == "tsx" || ext == "jsx")
}

pub fn log_change(
    source_path: &Path,
    added_file: usize,
    removed_file: usize,
    output_path: &Path,
    added_global: usize,
    removed_global: usize,
    time_us: u128,
) {
    if added_file == 0 && removed_file == 0 && added_global == 0 && removed_global == 0 {
        return;
    }

    let source_str = source_path
        .canonicalize()
        .unwrap_or_else(|_| source_path.to_path_buf())
        .display()
        .to_string();
    let output_str = output_path
        .canonicalize()
        .unwrap_or_else(|_| output_path.to_path_buf())
        .display()
        .to_string();

    let file_changes = format!(
        "({}, {})",
        format!("+{}", added_file).bright_green(),
        format!("-{}", removed_file).bright_red()
    );

    let output_changes = format!(
        "({}, {})",
        format!("+{}", added_global).bright_green(),
        format!("-{}", removed_global).bright_red()
    );

    let time_str = if time_us < 1000 {
        format!("{}µs", time_us)
    } else {
        format!("{:.2}ms", time_us as f64 / 1000.0)
    };

    println!(
        "{} {} -> {} {} · {}",
        source_str.bright_cyan(),
        file_changes,
        output_str.bright_magenta(),
        output_changes,
        time_str.yellow()
    );
}