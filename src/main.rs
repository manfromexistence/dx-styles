use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use colored::Colorize;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

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

    let dir = PathBuf::from("src");
    let output_file = PathBuf::from(".").join("styles.css");

    let mut file_classnames: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    let mut classname_counts: HashMap<String, u32> = HashMap::new();
    let mut global_classnames: HashSet<String> = HashSet::new();
    let mut pending_events: HashMap<PathBuf, Instant> = HashMap::new();

    let scan_start = Instant::now();
    let files = utils::find_code_files(&dir);
    if !files.is_empty() {
        let mut total_added_in_files = 0;
        for file in &files {
            let new_classnames = parser::parse_classnames(file);
            let (added, _, _, _) = data_manager::update_class_maps(file, &new_classnames, &mut file_classnames, &mut classname_counts, &mut global_classnames);
            total_added_in_files += added;
        }
        generator::generate_css(&global_classnames, &output_file, &style_engine);
        utils::log_change(&dir, total_added_in_files, 0, &output_file, global_classnames.len(), 0, scan_start.elapsed().as_micros());
    } else {
        println!("{}", "No .tsx or .jsx files found in src/.".yellow());
    }

    println!("{}", "Dx Styles is watching for file changes...".bold().cyan());

    let (tx, rx) = std::sync::mpsc::channel();
    let config = Config::default().with_poll_interval(Duration::from_millis(50));
    let mut watcher = RecommendedWatcher::new(tx, config).unwrap();
    watcher.watch(&dir, RecursiveMode::Recursive).unwrap();

    let mut event_queue: VecDeque<(PathBuf, bool)> = VecDeque::new();

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                for path in event.paths {
                    if utils::is_code_file(&path) {
                        let is_remove = matches!(event.kind, notify::EventKind::Remove(_));
                        event_queue.push_back((path, is_remove));
                    }
                }
            }
            Ok(Err(e)) => println!("Watch error: {:?}", e),
            Err(_) => {
                let mut processed_paths = HashSet::new();
                let now = Instant::now();
                while let Some((path, is_remove)) = event_queue.pop_front() {
                    if processed_paths.contains(&path) {
                        continue;
                    }
                    if let Some(last_time) = pending_events.get(&path) {
                        if now.duration_since(*last_time) < Duration::from_millis(100) {
                            event_queue.push_back((path.clone(), is_remove));
                            continue;
                        }
                    }
                    if is_remove {
                        watcher::process_file_remove(&path, &mut file_classnames, &mut classname_counts, &mut global_classnames, &output_file, &style_engine);
                    } else {
                        watcher::process_file_change(&path, &mut file_classnames, &mut classname_counts, &mut global_classnames, &output_file, &style_engine);
                    }
                    pending_events.insert(path.clone(), now);
                    processed_paths.insert(path);
                }
            }
        }
    }
}
