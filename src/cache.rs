use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use bincode::{deserialize, serialize};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
struct FileCache {
    modified: u64,
    classnames: HashSet<String>,
}

pub struct ClassnameCache {
    cache_dir: PathBuf,
    memory_cache: RwLock<HashMap<PathBuf, FileCache>>,
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
        if let Ok(entries) = fs::read_dir(cache_dir) {
            for entry in entries.filter_map(Result::ok) {
                if entry.path().extension().map_or(false, |ext| ext == "bin") {
                    if let Ok(data) = fs::read(entry.path()) {
                        if let Ok(file_cache) = deserialize(&data) {
                            let path = entry.path().with_extension("").with_extension("tsx");
                            cache.insert(path, file_cache);
                        }
                    }
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
        let cache_path = self.cache_dir.join(path.strip_prefix("src").unwrap_or(path).with_extension("bin"));
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
        
        let data = serialize(&file_cache)?;
        fs::write(&cache_path, data)?;
        
        Ok(())
    }
}