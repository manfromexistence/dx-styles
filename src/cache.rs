use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use flatbuffers::FlatBufferBuilder;

mod cache_generated {
    #![allow(dead_code, unused_imports, unsafe_op_in_unsafe_fn)]
    include!(concat!(env!("OUT_DIR"), "/dx_cache/cache_generated.rs"));
}

use cache_generated::{Cache, CacheArgs, FileEntry, FileEntryArgs};

pub struct ClassnameCache {
    cache_dir: PathBuf,
    memory_cache: RwLock<HashMap<PathBuf, FileCache>>,
}

#[derive(Clone)]
struct FileCache {
    modified: u64,
    classnames: HashSet<String>,
}

impl ClassnameCache {
    pub fn new(cache_dir: &str) -> Self {
        let cache_dir = PathBuf::from(cache_dir);
        fs::create_dir_all(&cache_dir).expect("Failed to create cache directory");
        
        let memory_cache = Self::load_from_disk(&cache_dir);
        
        Self {
            cache_dir,
            memory_cache: RwLock::new(memory_cache),
        }
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
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())?;
            
            if cached.modified == modified {
                return Some(cached.classnames.clone());
            }
        }
        None
    }

    pub fn set(&self, path: &Path, classnames: &HashSet<String>) -> Result<(), Box<dyn std::error::Error>> {
        let modified = fs::metadata(path)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        
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
}