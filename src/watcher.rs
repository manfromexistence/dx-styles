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