use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use colored::Colorize;
use notify::{Config, RecursiveMode};
use notify_debouncer_full::new_debouncer;
use rayon::prelude::*;
use crate::cache::ClassnameCache;
mod cache;
mod data_manager;
mod engine;
mod generator;
mod parser;
mod utils;
mod watcher;

fn main() {
    let styles_toml_path = PathBuf::from("styles.toml");
    let styles_bin_path = PathBuf::from(".dx/styles.bin");
    if !styles_toml_path.exists() {
        println!("{}", "styles.toml not found, creating default...".yellow());
        fs::write(&styles_toml_path, r#"
[static]
# Add static styles here
[dynamic]
# Add dynamic styles here
[generators]
# Add generators here
"#).expect("Failed to create styles.toml");
    }
    if !styles_bin_path.exists() {
        println!("{}", "styles.bin not found, running cargo build to generate it...".yellow());
        let output = std::process::Command::new("cargo")
            .arg("build")
            .output()
            .expect("Failed to run cargo build");
        if !output.status.success() {
            println!("{} Failed to generate styles.bin: {}", "Error:".red(), String::from_utf8_lossy(&output.stderr));
            return;
        }
        if !styles_bin_path.exists() {
            println!("{} styles.bin still not found after cargo build.", "Error:".red());
            return;
        }
    }

    if fs::metadata(&styles_bin_path).is_err() {
        println!("{} styles.bin is not accessible in .dx directory.", "Error:".red());
        return;
    }

    let style_engine = match engine::StyleEngine::new() {
        Ok(engine) => engine,
        Err(e) => {
            println!("{} Failed to initialize StyleEngine: {}. Ensure styles.bin in .dx is valid.", "Error:".red(), e);
            return;
        }
    };
    println!("{}", "✅ Dx Styles initialized with new Style Engine.".bold().green());

    let cache = ClassnameCache::new(".dx", "inspirations/website/app/globals.css");
    let dir = PathBuf::from("inspirations/website");
    let output_file = PathBuf::from("inspirations/website/app/globals.css");

    let mut file_classnames: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    let mut classname_counts: HashMap<String, u32> = HashMap::new();
    let mut global_classnames: HashSet<String> = HashSet::new();

    let scan_start = Instant::now();
    let files = utils::find_code_files(&dir);
    if !files.is_empty() {
        let results: Vec<_> = files.par_iter()
            .filter_map(|file| {
                let new_classnames = cache.compare_and_generate(file).expect("Failed to compare and generate classnames");
                if new_classnames.is_empty() {
                    None
                } else {
                    cache.update_from_classnames(file, &new_classnames).expect("Failed to update cache");
                    Some((file.to_path_buf(), new_classnames))
                }
            })
            .collect();
        let mut total_added_in_files = 0;
        let mut total_added_global = 0;
        for (file, new_classnames) in results {
            let start = Instant::now();
            let (added_file, removed_file, added_global, removed_global) = data_manager::update_class_maps(
                &file,
                &new_classnames,
                &mut file_classnames,
                &mut classname_counts,
                &mut global_classnames,
            );
            total_added_in_files += added_file;
            total_added_global += added_global;
            if removed_file > 0 || added_global > 0 {
                utils::log_change(
                    &file,
                    added_file,
                    removed_file,
                    &output_file,
                    added_global,
                    removed_global,
                    start.elapsed().as_micros(),
                );
            }
        }
        if (total_added_in_files > 0 || total_added_global > 0) && !global_classnames.is_empty() {
            generator::generate_css(&global_classnames, &output_file, &style_engine, &file_classnames);
            utils::log_change(
                &dir,
                total_added_in_files,
                0,
                &output_file,
                total_added_global,
                0,
                scan_start.elapsed().as_micros(),
            );
        }
    } else {
        println!("{}", "No .tsx or .jsx files found in inspirations/website/.".yellow());
    }

    println!("{}", "Dx Styles is watching for file changes...".bold().cyan());

    let (tx, rx) = mpsc::channel();
    let _config = Config::default().with_poll_interval(Duration::from_millis(50));
    let mut watcher = new_debouncer(Duration::from_millis(100), None, tx).expect("Failed to create watcher");
    watcher.watch(&dir, RecursiveMode::Recursive).expect("Failed to start watcher");

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                for event in events {
                    for path in &event.event.paths {
                        if utils::is_code_file(path) && *path != output_file {
                            if matches!(event.event.kind, notify::EventKind::Remove(_)) {
                                watcher::process_file_remove(path, &mut file_classnames, &mut classname_counts, &mut global_classnames, &output_file, &style_engine);
                            } else {
                                watcher::process_file_change(path, &mut file_classnames, &mut classname_counts, &mut global_classnames, &output_file, &style_engine);
                            }
                        }
                    }
                }
            }
            Ok(Err(e)) => println!("Watch error: {:?}", e),
            Err(e) => println!("Channel error: {:?}", e),
        }
    }
}