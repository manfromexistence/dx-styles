use std::collections::{HashMap, HashSet};
use crate::parser::parse_classnames;
use flatbuffers::FlatBufferBuilder;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use rayon::prelude::*;
use std::fs;

mod cache_generated {
    #![allow(dead_code, unused_imports, unsafe_op_in_unsafe_fn, mismatched_lifetime_syntaxes)]
    include!(concat!(env!("OUT_DIR"), "/cache/cache_generated.rs"));
}

use cache_generated::cache::{Cache, CacheArgs, FileEntry, FileEntryArgs};

pub struct ClassnameCache {
    cache_dir: PathBuf,
    memory_cache: RwLock<HashMap<PathBuf, FileCache>>,
    #[allow(dead_code)]
    css_path: PathBuf,
}

#[derive(Clone)]
struct FileCache {
    modified: u64,
    classnames: HashSet<String>,
}

impl ClassnameCache {
    pub fn new(cache_dir: &str, css_path: &str) -> Self {
        let cache_dir = PathBuf::from(cache_dir);
        let css_path = PathBuf::from(css_path);
        fs::create_dir_all(&cache_dir).expect("Failed to create cache directory");
        
        if !css_path.exists() {
            fs::write(&css_path, "").expect("Failed to create initial CSS file");
        }
        
        let memory_cache = if cache_dir.join("cache.bin").exists() {
            Self::load_from_disk(&cache_dir)
        } else {
            Self::build_cache_from_css(&cache_dir, &css_path)
        };
        
        Self {
            cache_dir,
            memory_cache: RwLock::new(memory_cache),
            css_path,
        }
    }

    fn build_cache_from_css(cache_dir: &Path, css_path: &Path) -> HashMap<PathBuf, FileCache> {
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
                        if let Ok(modified) = metadata.modified().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)).and_then(|m| m.duration_since(std::time::UNIX_EPOCH).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))).map(|d| d.as_secs()) {
                            let mut cache = cache.lock().unwrap();
                            cache.insert(path.clone(), FileCache {
                                modified,
                                classnames: intersection,
                            });
                        }
                    }
                }
            });

            let mut builder = FlatBufferBuilder::new();
            let cache = cache.lock().unwrap();
            let entries: Vec<_> = cache.iter()
                .map(|(path, cache)| {
                    let path_str = path.to_string_lossy().into_owned();
                    let path_offset = builder.create_string(&path_str);
                    let classnames: Vec<_> = cache.classnames.iter().map(|s| builder.create_string(s)).collect();
                    let classnames_vec = builder.create_vector(&classnames);
                    FileEntry::create(&mut builder, &FileEntryArgs {
                        path: Some(path_offset),
                        modified: cache.modified,
                        classnames: Some(classnames_vec),
                    })
                })
                .collect();
            
            let entries_vec = builder.create_vector(&entries);
            let cache_data = Cache::create(&mut builder, &CacheArgs {
                entries: Some(entries_vec),
            });
            
            builder.finish(cache_data, None);
            fs::write(cache_dir.join("cache.bin"), builder.finished_data()).expect("Failed to write cache.bin");
        }
        
        cache.into_inner().unwrap()
    }

    fn load_from_disk(cache_dir: &Path) -> HashMap<PathBuf, FileCache> {
        let mut cache = HashMap::new();
        let cache_path = cache_dir.join("cache.bin");
        if let Ok(data) = fs::read(&cache_path) {
            let cache_data = unsafe { flatbuffers::root_unchecked::<Cache>(&data) };
            if let Some(entries) = cache_data.entries() {
                for entry in entries {
                    let path = PathBuf::from(entry.path().unwrap_or_default());
                    let classnames = entry.classnames()
                        .unwrap_or_default()
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    cache.insert(path, FileCache {
                        modified: entry.modified(),
                        classnames,
                    });
                }
            }
        }
        cache
    }

    pub fn get(&self, path: &Path) -> Option<HashSet<String>> {
        let memory_cache = self.memory_cache.read().unwrap();
        if let Some(cached) = memory_cache.get(path) {
            let metadata = fs::metadata(path).ok()?;
            let modified = metadata
                .modified()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))
                .map(|d| d.as_secs())
                .ok()?;
            
            if cached.modified == modified {
                return Some(cached.classnames.clone());
            }
        }
        None
    }

    pub fn set(&self, path: &Path, classnames: &HashSet<String>) -> Result<(), Box<dyn std::error::Error>> {
        let modified = fs::metadata(path)?
            .modified()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))
            .map(|d| d.as_secs())?;
        
        let file_cache = FileCache {
            modified,
            classnames: classnames.clone(),
        };
        
        {
            let mut memory_cache = self.memory_cache.write().unwrap();
            memory_cache.insert(path.to_path_buf(), file_cache.clone());
        }
        
        let mut builder = FlatBufferBuilder::new();
        let entries: Vec<_> = {
            let memory_cache = self.memory_cache.read().unwrap();
            memory_cache.iter()
                .map(|(path, cache)| {
                    let path_str = path.to_string_lossy().into_owned();
                    let path_offset = builder.create_string(&path_str);
                    let classnames: Vec<_> = cache.classnames.iter()
                        .map(|s| builder.create_string(s))
                        .collect();
                    let classnames_vec = builder.create_vector(&classnames);
                    FileEntry::create(&mut builder, &FileEntryArgs {
                        path: Some(path_offset),
                        modified: cache.modified,
                        classnames: Some(classnames_vec),
                    })
                })
                .collect()
        };
        
        let entries_vec = builder.create_vector(&entries);
        let cache = Cache::create(&mut builder, &CacheArgs {
            entries: Some(entries_vec),
        });
        
        builder.finish(cache, None);
        fs::write(self.cache_dir.join("cache.bin"), builder.finished_data())?;
        
        Ok(())
    }

    pub fn update_from_classnames(&self, path: &Path, classnames: &HashSet<String>) -> Result<(), Box<dyn std::error::Error>> {
        self.set(path, classnames)
    }

    pub fn compare_and_generate(&self, path: &Path) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
        let cached_classnames = self.get(path).unwrap_or_default();
        let current_classnames = parse_classnames(path, self);
        
        let new_classnames: HashSet<String> = current_classnames
            .difference(&cached_classnames)
            .cloned()
            .collect();
        
        if !new_classnames.is_empty() {
            self.set(path, &current_classnames)?;
        }
        
        Ok(new_classnames)
    }
}