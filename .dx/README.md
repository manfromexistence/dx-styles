cache.rs
use std::collections::{HashMap, HashSet};
use crate::parser::parse_classnames;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use rayon::prelude::*;
use std::fs;
use rkyv::{Archive, Deserialize, Serialize as RkyvSerialize};

#[derive(Clone, Archive, RkyvSerialize, Deserialize)]
#[archive_attr(derive(Hash, Eq, PartialEq))]
struct FileCache {
    modified: u64,
    classnames: HashSet<String>,
}

pub struct ClassnameCache {
    cache_dir: PathBuf,
    memory_cache: RwLock<HashMap<String, FileCache>>,
    css_path: PathBuf,
}

impl ClassnameCache {
    pub fn new(cache_dir: &str, css_path: &str) -> Self {
        let cache_dir_path = PathBuf::from(cache_dir);
        let css_path_buf = PathBuf::from(css_path);
        fs::create_dir_all(&cache_dir_path).expect("Failed to create cache directory");

        if !css_path_buf.exists() {
            fs::write(&css_path_buf, "").expect("Failed to create initial CSS file");
        }

        let memory_cache = if cache_dir_path.join("cache.bin").exists() {
            Self::load_from_disk(&cache_dir_path)
        } else {
            Self::build_cache_from_css(&cache_dir_path, &css_path_buf)
        };

        Self {
            cache_dir: cache_dir_path,
            memory_cache: RwLock::new(memory_cache),
            css_path: css_path_buf,
        }
    }

    fn build_cache_from_css(cache_dir: &Path, css_path: &Path) -> HashMap<String, FileCache> {
        let mut cache = Mutex::new(HashMap::new());

        if css_path.exists() {
            let css_content = fs::read_to_string(css_path).unwrap_or_default();
            let css_classnames: HashSet<String> = css_content
                .lines()
                .filter_map(|line| {
                    line.trim().strip_prefix('.').and_then(|s| s.split('{').next()).map(str::to_string)
                })
                .collect();

            let tsx_files: Vec<PathBuf> = walkdir::WalkDir::new("playgrounds/nextjs")
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "tsx"))
                .map(|e| e.path().to_path_buf())
                .collect();

            tsx_files.par_iter().for_each(|path| {
                let classnames = parse_classnames(path, &Self {
                    cache_dir: cache_dir.to_path_buf(),
                    memory_cache: RwLock::new(HashMap::new()),
                    css_path: css_path.to_path_buf(),
                });
                let intersection: HashSet<String> = classnames
                    .intersection(&css_classnames)
                    .cloned()
                    .collect();
                if !intersection.is_empty() {
                    if let Ok(metadata) = fs::metadata(path) {
                        if let Ok(modified) = metadata.modified().and_then(|m| m.duration_since(std::time::UNIX_EPOCH)).map(|d| d.as_secs()) {
                            let path_str = path.to_string_lossy().into_owned();
                            let mut cache_guard = cache.lock().unwrap();
                            cache_guard.insert(path_str, FileCache {
                                modified,
                                classnames: intersection,
                            });
                        }
                    }
                }
            });
        }

        let cache_data = cache.into_inner().unwrap();
        let bytes = rkyv::to_bytes::<_, 4096>(&cache_data).unwrap();
        fs::write(cache_dir.join("cache.bin"), bytes).expect("Failed to write cache.bin");

        cache_data
    }

    fn load_from_disk(cache_dir: &Path) -> HashMap<String, FileCache> {
        let cache_path = cache_dir.join("cache.bin");
        if let Ok(data) = fs::read(&cache_path) {
            if let Ok(cache_data) = rkyv::from_bytes::<HashMap<String, FileCache>>(&data) {
                return cache_data;
            }
        }
        HashMap::new()
    }

    pub fn get(&self, path: &Path) -> Option<HashSet<String>> {
        let path_str = path.to_string_lossy().into_owned();
        let memory_cache = self.memory_cache.read().unwrap();
        if let Some(cached) = memory_cache.get(&path_str) {
            if let Ok(metadata) = fs::metadata(path) {
                if let Ok(modified) = metadata.modified().and_then(|m| m.duration_since(std::time::UNIX_EPOCH)).map(|d| d.as_secs()) {
                    if cached.modified == modified {
                        return Some(cached.classnames.clone());
                    }
                }
            }
        }
        None
    }

    pub fn set(&self, path: &Path, classnames: &HashSet<String>) -> Result<(), Box<dyn std::error::Error>> {
        let path_str = path.to_string_lossy().into_owned();
        let modified = if path.exists() {
            fs::metadata(path)?
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs()
        } else {
            0
        };

        let file_cache = FileCache {
            modified,
            classnames: classnames.clone(),
        };

        {
            let mut memory_cache = self.memory_cache.write().unwrap();
            memory_cache.insert(path_str, file_cache);
        }

        self.save_to_disk()?;

        Ok(())
    }

    fn save_to_disk(&self) -> Result<(), Box<dyn std::error::Error>> {
        let memory_cache = self.memory_cache.read().unwrap();
        let bytes = rkyv::to_bytes::<_, 4096>(&*memory_cache).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        fs::write(self.cache_dir.join("cache.bin"), bytes)?;
        Ok(())
    }

    pub fn remove(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let path_str = path.to_string_lossy().into_owned();
        {
            let mut memory_cache = self.memory_cache.write().unwrap();
            memory_cache.remove(&path_str);
        }
        self.save_to_disk()?;
        Ok(())
    }

    pub fn compare_and_generate(&self, path: &Path) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
        if self.get(path).is_some() {
            return Ok(HashSet::new());
        }
        let current_classnames = parse_classnames(path, self);
        self.set(path, &current_classnames)?;
        Ok(current_classnames)
    }
}

main.rs
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
    "#).expect("Failed to create styles.toml!");
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

    let output_file = PathBuf::from("playgrounds/nextjs/app/globals.css");
    let cache = ClassnameCache::new(".dx", output_file.to_str().unwrap());
    let dir = PathBuf::from("playgrounds/nextjs");

    let mut file_classnames: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    let mut classname_counts: HashMap<String, u32> = HashMap::new();
    let mut global_classnames: HashSet<String> = HashSet::new();

    {
        let memory_cache = cache.memory_cache.read().unwrap();
        for (path_str, fc) in memory_cache.iter() {
            let path = PathBuf::from(path_str);
            file_classnames.insert(path.clone(), fc.classnames.clone());
            for cn in &fc.classnames {
                *classname_counts.entry(cn.clone()).or_insert(0) += 1;
                global_classnames.insert(cn.clone());
            }
        }
    }

    let scan_start = Instant::now();
    let files = utils::find_code_files(&dir);
    if !files.is_empty() {
        let results: Vec<_> = files.par_iter()
            .filter_map(|file| {
                let current_classnames = cache.compare_and_generate(file).ok()?;
                Some((file.clone(), current_classnames))
            })
            .collect();

        let mut total_added_in_files = 0;
        let mut total_removed_in_files = 0;
        let mut total_added_global = 0;
        let mut total_removed_global = 0;

        for (file, current_classnames) in results {
            let start = Instant::now();
            let (added_file, removed_file, added_global, removed_global) = data_manager::update_class_maps(
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
        if (total_added_global > 0 || total_removed_global > 0) && !global_classnames.is_empty() {
            generator::generate_css(&global_classnames, &output_file, &style_engine, &file_classnames);
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
        println!("{}", "No .tsx or .jsx files found in playgrounds/nextjs/.".yellow());
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
                                watcher::process_file_remove(&cache, path, &mut file_classnames, &mut classname_counts, &mut global_classnames, &output_file, &style_engine);
                            } else {
                                watcher::process_file_change(&cache, path, &mut file_classnames, &mut classname_counts, &mut global_classnames, &output_file, &style_engine);
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

watcher.rs
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;
use crate::{cache::ClassnameCache, data_manager, engine::StyleEngine, generator, utils};

pub fn process_file_change(
    cache: &ClassnameCache,
    path: &Path,
    file_classnames: &mut HashMap<PathBuf, HashSet<String>>,
    classname_counts: &mut HashMap<String, u32>,
    global_classnames: &mut HashSet<String>,
    output_path: &Path,
    engine: &StyleEngine,
) {
    let start = Instant::now();
    let current_classnames = match cache.compare_and_generate(path) {
        Ok(names) => names,
        Err(_) => return,
    };

    let (added_file, removed_file, added_global, removed_global) = data_manager::update_class_maps(
        path,
        &current_classnames,
        file_classnames,
        classname_counts,
        global_classnames,
    );

    if added_global > 0 || removed_global > 0 {
        generator::generate_css(global_classnames, output_path, engine, file_classnames);
        utils::log_change(
            path,
            added_file,
            removed_file,
            output_path,
            added_global,
            removed_global,
            start.elapsed().as_micros(),
        );
    }
}

pub fn process_file_remove(
    cache: &ClassnameCache,
    path: &Path,
    file_classnames: &mut HashMap<PathBuf, HashSet<String>>,
    classname_counts: &mut HashMap<String, u32>,
    global_classnames: &mut HashSet<String>,
    output_path: &Path,
    engine: &StyleEngine,
) {
    if !file_classnames.contains_key(path) {
        return;
    }

    let start = Instant::now();
    let empty_classnames = HashSet::new();

    let (added_file, removed_file, added_global, removed_global) = data_manager::update_class_maps(
        path,
        &empty_classnames,
        file_classnames,
        classname_counts,
        global_classnames,
    );

    cache.remove(path).ok();

    if added_global > 0 || removed_global > 0 {
        generator::generate_css(global_classnames, output_path, engine, file_classnames);
        utils::log_change(
            path,
            added_file,
            removed_file,
            output_path,
            added_global,
            removed_global,
            start.elapsed().as_micros(),
        );
    }
}

Cargo.toml
[package]
name = "dx"
version = "0.1.0"
edition = "2024"

[dependencies]
colored = "2.1.0"
lightningcss = "1.25.1"
lru = "0.12.4"
notify = "6.1.1"
notify-debouncer-full = "0.3.1"
oxc_allocator = "0.25.0"
oxc_ast = "0.25.0"
oxc_parser = "0.25.0"
oxc_span = "0.25.0"
walkdir = "2.5.0"
crossbeam-deque = "0.8.5"
futures = "0.3.30"
memmap2 = "0.9.4"
rayon = "1.10.0"
flatbuffers = "24.3.25"
rkyv = { version = "0.7.44", features = ["alloc", "std", "validation", "hashbrown"] }
serde = { version = "1.0.210", features = ["derive"] }

[build-dependencies]
flatc-rust = "0.2.0"
toml = "0.8.19"
flatbuffers = "24.3.25"
cc = "1.1.15"
serde = { version = "1.0.210", features = ["derive"] }

build.rs
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use flatbuffers::{FlatBufferBuilder, WIPOffset};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct TomlConfig {
    #[serde(rename = "static", default)]
    static_styles: HashMap<String, String>,
    #[serde(default)]
    dynamic: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    generators: HashMap<String, GeneratorConfig>,
}

#[derive(Deserialize, Debug, Clone)]
struct GeneratorConfig {
    multiplier: f32,
    unit: String,
}

#[derive(Debug, Clone)]
struct StyleRecord {
    name: String,
    css: String,
}

fn main() {
    let fbs_files = ["styles.fbs"];
    let toml_path = "styles.toml";
    let out_dir = std::env::var("OUT_DIR").unwrap();

    for fbs_file in fbs_files.iter() {
        println!("cargo:rerun-if-changed={}", fbs_file);
    }
    println!("cargo:rerun-if-changed={}", toml_path);

    flatc_rust::run(flatc_rust::Args {
        lang: "rust",
        inputs: &fbs_files.iter().map(|s| Path::new(s)).collect::<Vec<_>>(),
        out_dir: Path::new(&out_dir),
        includes: &[Path::new("src")],
        ..Default::default()
    })
    .expect("flatc schema compilation failed");

    let toml_content = fs::read_to_string(toml_path).expect("Failed to read styles.toml");
    let toml_data: TomlConfig = toml::from_str(&toml_content).expect("Failed to parse styles.toml");

    for (key, _values) in &toml_data.dynamic {
        let parts: Vec<&str> = key.split('|').collect();
        if parts.len() != 2 {
            panic!("Invalid dynamic key format in styles.toml: '{}'. Expected 'prefix|property'.", key);
        }
    }

    let mut precompiled_styles = Vec::new();

    for (name, css) in toml_data.static_styles {
        precompiled_styles.push(StyleRecord { name, css });
    }

    for (key, values) in toml_data.dynamic {
        let parts: Vec<&str> = key.split('|').collect();
        let prefix = parts[0];
        let property = parts[1];
        for (suffix, value) in values {
            let name = format!("{}-{}", prefix, suffix);
            let css = format!("{}: {};", property, value);
            precompiled_styles.push(StyleRecord { name, css });
        }
    }

    precompiled_styles.sort_by(|a, b| a.name.cmp(&b.name));

    let mut builder = FlatBufferBuilder::new();

    let mut style_offsets = Vec::new();
    for style in &precompiled_styles {
        let name_offset = builder.create_string(&style.name);
        let css_offset = builder.create_string(&style.css);
        
        let table_wip = builder.start_table();
        builder.push_slot(4, name_offset, WIPOffset::new(0));
        builder.push_slot(6, css_offset, WIPOffset::new(0));
        let style_offset = builder.end_table(table_wip);
        style_offsets.push(style_offset);
    }
    let styles_vec = builder.create_vector(&style_offsets);

    let mut generator_offsets = Vec::new();
    for (key, config) in toml_data.generators {
        let parts: Vec<&str> = key.split('|').collect();
        if parts.len() != 2 { continue; }
        
        let prefix_offset = builder.create_string(parts[0]);
        let property_offset = builder.create_string(parts[1]);
        let unit_offset = builder.create_string(&config.unit);

        let table_wip = builder.start_table();
        builder.push_slot(4, prefix_offset, WIPOffset::new(0));
        builder.push_slot(6, property_offset, WIPOffset::new(0));
        builder.push_slot(8, config.multiplier, 0.0f32);
        builder.push_slot(10, unit_offset, WIPOffset::new(0));
        let gen_offset = builder.end_table(table_wip);
        generator_offsets.push(gen_offset);
    }
    let generators_vec = builder.create_vector(&generator_offsets);

    let table_wip = builder.start_table();
    builder.push_slot(4, styles_vec, WIPOffset::new(0));
    builder.push_slot(6, generators_vec, WIPOffset::new(0));
    let config_root = builder.end_table(table_wip);

    builder.finish(config_root, None);

    let buf = builder.finished_data();
    let styles_bin_path = Path::new(".dx/styles.bin");
    fs::create_dir_all(styles_bin_path.parent().unwrap()).expect("Failed to create .dx directory");
    fs::write(styles_bin_path, buf).expect("Failed to write styles.bin");

    println!("✅ Successfully generated .dx/styles.bin from styles.toml");
}

To make dx-styles better: make directories configurable via env vars or config file; integrate with build tools like webpack for automatic watching; add support for more file types like .js/.vue; optimize generator by precomputing common classes; add tests for cache consistency and parser accuracy; use tracing for performance profiling.