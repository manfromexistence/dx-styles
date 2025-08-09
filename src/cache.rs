use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct FileCache {
    modified: u64,
    classnames: HashSet<String>,
}

pub struct ClassnameCache {
    cache_dir: PathBuf,
}

impl ClassnameCache {
    pub fn new(cache_dir: &str) -> Self {
        let cache_dir = PathBuf::from(cache_dir);
        fs::create_dir_all(&cache_dir).expect("Failed to create cache directory");
        Self { cache_dir }
    }

    pub fn get(&self, path: &Path) -> Option<HashSet<String>> {
        let cache_path = self.cache_dir.join(path.strip_prefix("src").unwrap_or(path).with_extension("json"));
        let metadata = fs::metadata(path).ok()?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())?;

        let file = File::open(&cache_path).ok()?;
        let cache: FileCache = serde_json::from_reader(&file).ok()?;
        if cache.modified == modified {
            Some(cache.classnames)
        } else {
            None
        }
    }

    pub fn set(&self, path: &Path, classnames: &HashSet<String>) -> Result<(), Box<dyn std::error::Error>> {
        let cache_path = self.cache_dir.join(path.strip_prefix("src").unwrap_or(path).with_extension("json"));
        let modified = fs::metadata(path)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let cache = FileCache {
            modified,
            classnames: classnames.clone(),
        };
        let mut file = File::create(&cache_path)?;
        serde_json::to_writer_pretty(&mut file, &cache)?;
        file.flush()?;
        Ok(())
    }
}