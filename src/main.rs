use std::collections::{HashMap, HashSet};
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
    let style_engine = match engine::StyleEngine::new() {
        Ok(engine) => engine,
        Err(e) => {
            println!("{} Failed to initialize StyleEngine: {}. Please run 'cargo build' to generate it.", "Error:".red(), e);
            return;
        }
    };
    println!("{}", "✅ Dx Styles initialized with new Style Engine.".bold().green());

    let cache = ClassnameCache::new(".dx_cache");
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
                let new_classnames = parser::parse_classnames(file, &cache);
                if new_classnames.is_empty() {
                    None
                } else {
                    Some((file, new_classnames))
                }
            })
            .collect();
        let mut total_added_in_files = 0;
        for (file, new_classnames) in results {
            let (added, _, _, _) = data_manager::update_class_maps(file, &new_classnames, &mut file_classnames, &mut classname_counts, &mut global_classnames);
            total_added_in_files += added;
        }
        generator::generate_css(&global_classnames, &output_file, &style_engine, &file_classnames);
        utils::log_change(&dir, total_added_in_files, 0, &output_file, global_classnames.len(), 0, scan_start.elapsed().as_micros());
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