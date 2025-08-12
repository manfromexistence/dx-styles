cache.rs
use std::collections::{HashMap, HashSet};
use crate::parser::parse_classnames;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use rayon::prelude::*;
use std::fs;
use rkyv::{Archive, Deserialize, Serialize as RkyvSerialize};

#[derive(Clone, Archive, RkyvSerialize, rkyv::Deserialize)]
#[archive(derive(Hash, Eq, PartialEq))]
struct FileCache {
    modified: u64,
    classnames: HashSet<String>,
}

pub struct ClassnameCache {
    cache_dir: PathBuf,
    pub memory_cache: RwLock<HashMap<String, FileCache>>,
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
        let cache = Mutex::new(HashMap::new());

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
                        if let Ok(modified) = metadata.modified().and_then(|m| Ok(m.duration_since(std::time::UNIX_EPOCH)?)).map(|d| d.as_secs()) {
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
        let bytes = rkyv::to_bytes(&cache_data).unwrap();
        fs::write(cache_dir.join("cache.bin"), bytes).expect("Failed to write cache.bin");

        cache_data
    }

    fn load_from_disk(cache_dir: &Path) -> HashMap<String, FileCache> {
        let cache_path = cache_dir.join("cache.bin");
        if let Ok(data) = fs::read(&cache_path) {
            if let Ok(cache_data) = rkyv::from_bytes(&data) {
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
                if let Ok(modified) = metadata.modified().and_then(|m| Ok(m.duration_since(std::time::UNIX_EPOCH)?)).map(|d| d.as_secs()) {
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
        let bytes = rkyv::to_bytes(&*memory_cache).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
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

watcher.rs
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
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