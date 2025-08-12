mod cache;
mod data_manager;
mod engine;
mod generator;
mod parser;
mod utils;
mod watcher;

use crate::cache::ClassnameCache;
use colored::Colorize;
use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use rayon::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    process,
    sync::mpsc,
    time::{Duration, Instant},
};

fn main() {
    let styles_toml_path = PathBuf::from("styles.toml");
    let styles_bin_path = PathBuf::from(".dx/styles.bin");

    if !styles_toml_path.exists() {
        println!("{}", "styles.toml not found, creating default...".yellow());
        fs::write(
            &styles_toml_path,
            r#"[static]
            [dynamic]
            [generators]"#,
        )
        .expect("Failed to create styles.toml!");
    }

    if !styles_bin_path.exists() {
        println!(
            "{}",
            "styles.bin not found, running cargo build...".yellow()
        );
        let output = std::process::Command::new("cargo")
            .arg("build")
            .output()
            .expect("Failed to run cargo build");
        if !output.status.success() {
            eprintln!(
                "{} Failed to generate styles.bin: {}",
                "Error:".red(),
                String::from_utf8_lossy(&output.stderr)
            );
            process::exit(1);
        }
    }

    let style_engine = match engine::StyleEngine::new() {
        Ok(engine) => engine,
        Err(e) => {
            eprintln!(
                "{} Failed to initialize StyleEngine: {}. Ensure styles.bin is valid.",
                "Error:".red(),
                e
            );
            process::exit(1);
        }
    };
    println!(
        "{}",
        "✅ Dx Styles initialized with new Style Engine."
            .bold()
            .green()
    );

    let output_file = PathBuf::from("playgrounds/nextjs/app/globals.css");
    let cache = match ClassnameCache::new(".dx/cache") {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Failed to open cache database: {}", "Error:".red(), e);
            process::exit(1);
        }
    };
    let dir = PathBuf::from("playgrounds/nextjs");

    let mut file_classnames: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    let mut classname_counts: HashMap<String, u32> = HashMap::new();
    let mut global_classnames: HashSet<String> = HashSet::new();

    for (path, fc) in cache.iter() {
        file_classnames.insert(path, fc.classnames.clone());
        for cn in &fc.classnames {
            *classname_counts.entry(cn.clone()).or_insert(0) += 1;
            global_classnames.insert(cn.clone());
        }
    }

    let scan_start = Instant::now();
    let files = utils::find_code_files(&dir);
    if !files.is_empty() {
        let results: Vec<_> = files
            .par_iter()
            .filter_map(|file| match cache.compare_and_generate(file) {
                Ok(Some(classnames)) => Some((file.clone(), classnames)),
                _ => None,
            })
            .collect();

        let mut total_added_in_files = 0;
        let mut total_removed_in_files = 0;
        let mut total_added_global = 0;
        let mut total_removed_global = 0;

        for (file, current_classnames) in results {
            let start = Instant::now();
            let (added_file, removed_file, added_global, removed_global) =
                data_manager::update_class_maps(
                    &file,
                    &current_classnames,
                    &mut file_classnames,
                    &mut classname_counts,
                    &mut global_classnames,
                );
            total_added_in_files += added_file;
            total_removed_in_files += removed_file;
            total_added_global += added_global;
            total_removed_global += removed_global;
            if added_file > 0 || removed_file > 0 {
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
        if (total_added_global > 0 || total_removed_global > 0) || !global_classnames.is_empty() {
            generator::generate_css(
                &global_classnames,
                &output_file,
                &style_engine,
                &file_classnames,
            );
            utils::log_change(
                &dir,
                total_added_in_files,
                total_removed_in_files,
                &output_file,
                total_added_global,
                total_removed_global,
                scan_start.elapsed().as_micros(),
            );
        }
    } else {
        println!(
            "{}",
            "No .tsx or .jsx files found in playgrounds/nextjs/.".yellow()
        );
    }

    println!(
        "{}",
        "Dx Styles is watching for file changes...".bold().cyan()
    );

    let (tx, rx) = mpsc::channel();
    let mut watcher =
        new_debouncer(Duration::from_millis(50), None, tx).expect("Failed to create watcher");
    watcher
        .watch(&dir, RecursiveMode::Recursive)
        .expect("Failed to start watcher");

    for res in rx {
        match res {
            Ok(events) => {
                for event in events {
                    for path in &event.paths {
                        if utils::is_code_file(path) && *path != output_file {
                            if matches!(event.kind, notify::event::EventKind::Remove(_)) {
                                watcher::process_file_remove(
                                    &cache,
                                    path,
                                    &mut file_classnames,
                                    &mut classname_counts,
                                    &mut global_classnames,
                                    &output_file,
                                    &style_engine,
                                );
                            } else {
                                watcher::process_file_change(
                                    &cache,
                                    path,
                                    &mut file_classnames,
                                    &mut classname_counts,
                                    &mut global_classnames,
                                    &output_file,
                                    &style_engine,
                                );
                            }
                        }
                    }
                }
            }
            Err(e) => println!("Watch error: {:?}", e),
        }
    }
}